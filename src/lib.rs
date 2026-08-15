//! rmudos — a concise Rust LPC driver inspired by MudOS / FluffOS.

pub mod backend;
pub mod compiler;
pub mod config;
pub mod efun;
pub mod net;
pub mod simulate;
pub mod vm;

pub use config::DriverConfig;
pub use vm::MudWorld;

#[cfg(test)]
mod compile_smoke {
    use super::*;

    #[test]
    fn compile_mudlib() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        for path in [
            "/secure/master",
            "/std/room",
            "/std/user",
            "/room/void",
            "/room/tavern",
            "/room/street",
        ] {
            compiler::compile_file_in(&mudlib, path)
                .unwrap_or_else(|e| panic!("{path}: {e:#}"));
        }
    }
}

#[cfg(test)]
mod runtime_smoke {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn boot_master_only() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        assert!(world.find_object("/room/void").is_some());
    }

    #[test]
    fn connect_sends_welcome() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let peer = "127.0.0.1:9".parse().unwrap();
        let player = simulate::connect_player(&world, peer, tx).expect("connect");
        let mut out = String::new();
        while let Ok(msg) = rx.try_recv() {
            out.push_str(&msg);
            out.push('\n');
        }
        eprintln!("OUT:\n{out}");
        assert!(out.contains("Welcome") || out.contains("Void"), "{out}");
        assert!(player.lock().interactive.is_some());
    }
}
