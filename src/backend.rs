//! Driver backend — accept loop, input dispatch, heartbeats.

use crate::net::{TelnetOut, TelnetSession};
use crate::simulate;
use crate::vm::MudWorld;
use anyhow::Result;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tracing::{error, info};

pub async fn run(world: Arc<MudWorld>) -> Result<()> {
    simulate::boot_master(&world)?;
    info!(
        mud = %world.config.mud_name,
        master = %world.config.master,
        "master object booted"
    );

    let addr = world.config.socket_address();
    let listener = TcpListener::bind(&addr).await?;
    info!(%addr, "listening for players (telnet/line mode)");

    let world_hb = world.clone();
    tokio::spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
        let mut reset_tick = 0u64;
        loop {
            tick.tick().await;
            reset_tick += 1;
            let do_reset = reset_tick % 30 == 0;
            let world = world_hb.clone();
            // LPC must not run on the tokio worker concurrently with player input.
            let _ = tokio::task::spawn_blocking(move || {
                let _eval = world.lock_eval();
                world.process_call_outs();
                world.heartbeat();
                if do_reset {
                    world.process_resets();
                }
            })
            .await;
            if world_hb.is_shutting_down() {
                break;
            }
        }
    });

    loop {
        if world.is_shutting_down() {
            break;
        }
        let (socket, peer) = listener.accept().await?;
        let world = world.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_connection(world, socket, peer).await {
                error!(%peer, error = format!("{e:#}"), "connection ended with error");
            }
        });
    }
    Ok(())
}

async fn handle_connection(
    world: Arc<MudWorld>,
    socket: tokio::net::TcpStream,
    peer: std::net::SocketAddr,
) -> Result<()> {
    let (tx, rx) = mpsc::unbounded_channel::<TelnetOut>();
    let (line_tx, mut line_rx) = mpsc::unbounded_channel::<String>();

    // Start the writer before logon so welcome text is flushed.
    let io = tokio::spawn(async move {
        TelnetSession::run(socket, rx, move |line| {
            let _ = line_tx.send(line);
        })
        .await
    });

    // Yield so the IO task can attach to the socket before we emit text.
    tokio::task::yield_now().await;

    let world_connect = world.clone();
    let player = match tokio::task::spawn_blocking(move || {
        let _eval = world_connect.lock_eval();
        simulate::connect_player(&world_connect, peer, tx)
    })
    .await
    {
        Ok(Ok(player)) => player,
        Ok(Err(error)) => {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(100), io).await;
            return Err(error);
        }
        Err(error) => {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(100), io).await;
            return Err(anyhow::anyhow!(error));
        }
    };
    // Keep the Interactive Arc so we can follow `exec()` onto /std/user.
    let interactive = player
        .lock()
        .interactive
        .clone()
        .ok_or_else(|| anyhow::anyhow!("connect did not attach interactive"))?;

    while let Some(line) = line_rx.recv().await {
        let Some(player) = find_interactive_owner(&world, &interactive) else {
            // `exec()` reattaches before this loop runs again. None means
            // quit/destruct — keep waiting and the telnet session hangs.
            break;
        };
        if player.lock().destructed {
            break;
        }
        let world_input = world.clone();
        let player_input = player.clone();
        // LPC apply is sync and can be long; do not block the tokio worker.
        // eval_lock: only one LPC evaluation in the process (MudOS single-thread).
        match tokio::task::spawn_blocking(move || {
            let _eval = world_input.lock_eval();
            tracing::debug!("player input start");
            let result = world_input.handle_player_input(player_input, line);
            tracing::debug!("player input end");
            result
        })
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => {
                player.lock().write(format!("Error: {e:#}"));
                tracing::warn!(error = format!("{e:#}"), "process_input failed");
            }
            Err(e) => {
                tracing::warn!(error = format!("{e:#}"), "process_input join failed");
            }
        }
        // `quit` destructs the player; do not wait for another line.
        if find_interactive_owner(&world, &interactive).is_none() {
            break;
        }
    }

    if let Some(player) = find_interactive_owner(&world, &interactive) {
        let _eval = world.lock_eval();
        player.lock().interactive = None;
        let _ = simulate::destruct_object(&world, player);
    }
    drop(interactive);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(250), io).await;
    Ok(())
}

fn find_interactive_owner(
    world: &MudWorld,
    interactive: &Arc<crate::vm::object::Interactive>,
) -> Option<crate::vm::ObjectRef> {
    world.users().into_iter().find(|object| {
        object
            .lock()
            .interactive
            .as_ref()
            .is_some_and(|current| Arc::ptr_eq(current, interactive))
    })
}
