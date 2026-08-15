use anyhow::Result;
use rmudos::{backend, config::DriverConfig, vm::MudWorld};
use std::path::PathBuf;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

fn parse_args() -> (Option<PathBuf>, Option<u16>, Option<PathBuf>) {
    let mut config = None;
    let mut port = None;
    let mut mudlib = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-c" | "--config" => config = args.next().map(PathBuf::from),
            "-p" | "--port" => port = args.next().and_then(|s| s.parse().ok()),
            "--mudlib" => mudlib = args.next().map(PathBuf::from),
            "-h" | "--help" => {
                eprintln!(
                    "rmudos — Rust MudOS-inspired LPC driver\n\n\
                     Usage: rmudos [--config path] [--port N] [--mudlib dir]"
                );
                std::process::exit(0);
            }
            other => {
                eprintln!("unknown argument: {other}");
                std::process::exit(2);
            }
        }
    }
    (config, port, mudlib)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("rmudos=info".parse()?))
        .init();

    let (config_path, port, mudlib) = parse_args();
    let mut cfg = match &config_path {
        Some(path) => DriverConfig::from_file(path)?,
        None => DriverConfig::default(),
    };
    if let Some(port) = port {
        cfg.port = port;
    }
    if let Some(mudlib) = mudlib {
        cfg.mudlib = mudlib;
    }

    tracing::info!(
        mud = %cfg.mud_name,
        mudlib = %cfg.mudlib.display(),
        "starting rmudos"
    );

    let world = Arc::new(MudWorld::new(cfg));
    backend::run(world).await
}
