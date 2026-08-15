use crate::backend::{Heartbeat, Swap};
use crate::comm;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio::time::{self, Duration};
use log::{info, error};

use crate::globals::GLOBALS;

#[tokio::main]
pub async fn run() {
    env_logger::init();
    info!("MUD server starting up...");

    // Initialize global variables
    {
        let mut globals = GLOBALS.lock().unwrap();
        globals.boot_time = chrono::Utc::now().timestamp();
        for i in 0..10 {
            globals.consts[i] = (-(i as f64) / 900.0).exp();
        }
    }

    println!("Initialization complete.");
    println!("Starting backend loop...");

    let listener = TcpListener::bind("127.0.0.1:6666").await.unwrap();
    let shared = Arc::new(comm::Shared::new());
    let heartbeat = Arc::new(Heartbeat::new());
    let swap = Arc::new(Swap::new());

    let mut hb_interval = time::interval(Duration::from_secs(1));
    let mut swap_interval = time::interval(Duration::from_secs(60));

    loop {
        tokio::select! {
            Ok((socket, addr)) = listener.accept() => {
                let shared = shared.clone();
                let heartbeat = heartbeat.clone();
                tokio::spawn(async move {
                    heartbeat.set_heartbeat(addr, true);
                    comm::handle_connection(socket, shared, addr).await;
                    heartbeat.set_heartbeat(addr, false);
                });
            }
            _ = hb_interval.tick() => {
                heartbeat.call_heartbeat();
            }
            _ = swap_interval.tick() => {
                swap.look_for_objects_to_swap();
            }
        }
    }
}
