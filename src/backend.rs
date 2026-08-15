//! Driver backend — accept loop, input dispatch, heartbeats.

use crate::net::TelnetSession;
use crate::simulate;
use crate::vm::value::LpcValue;
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
        loop {
            tick.tick().await;
            world_hb.heartbeat();
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
                error!(%peer, error = %e, "connection ended with error");
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
    let (tx, rx) = mpsc::unbounded_channel::<String>();
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

    let player = match simulate::connect_player(&world, peer, tx) {
        Ok(player) => player,
        Err(error) => {
            let _ = tokio::time::timeout(std::time::Duration::from_millis(100), io).await;
            return Err(error);
        }
    };
    let player_id = player.lock().id;

    while let Some(line) = line_rx.recv().await {
        let Some(player) = world.object(player_id) else {
            break;
        };
        if player.lock().destructed {
            break;
        }
        match world.apply(
            player.clone(),
            "process_input",
            vec![LpcValue::String(line)],
            Some(player.clone()),
            None,
        ) {
            Ok(LpcValue::Int(0)) => {
                player.lock().interactive = None;
                break;
            }
            Ok(_) => {}
            Err(e) => {
                player.lock().write(format!("Error: {e}"));
                tracing::warn!(error = %e, "process_input failed");
            }
        }
        if player.lock().destructed || player.lock().interactive.is_none() {
            break;
        }
    }

    if let Some(player) = world.object(player_id) {
        player.lock().interactive = None;
        let _ = simulate::destruct_object(&world, player);
    }
    let _ = tokio::time::timeout(std::time::Duration::from_millis(250), io).await;
    Ok(())
}
