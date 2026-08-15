# RMUDOS

Rust MudOS driver for LPC. This is the Rust port started in [maadOS-rust](https://github.com/maadone74/maadOS-rust) (`rust-src/`) and published here as its own repository.

MudOS is an LPmud server (driver). RMUDOS is an in-progress rewrite of that driver in Rust, using Tokio for networking.

## Status

Early prototype. The current binary:

- Listens on `127.0.0.1:6666`
- Accepts telnet-style connections
- Prompts for username and password
- Echoes commands after a successful login
- Runs a heartbeat loop and a swap-scan stub

A hardcoded test account is included:

- Username: `testuser`
- Password: `password123`

LPC compilation, object runtime, and mudlib loading are not implemented yet.

## Build and run

```bash
cargo run --bin rust_mud_driver
```

Connect with:

```bash
telnet 127.0.0.1 6666
```

Or:

```bash
nc 127.0.0.1 6666
```

Set `RUST_LOG=info` for connection and login logs.

## Tests

The login tests expect a running server on port 6666:

```bash
cargo run --bin rust_mud_driver
cargo test --test login -- --ignored
```

The checked-in tests currently require a live driver process; they are not self-contained unit tests.

## Layout

| Path | Role |
| --- | --- |
| `src/main.rs` | Binary entry point |
| `src/main_logic.rs` | Boot, TCP accept loop, heartbeat/swap timers |
| `src/comm.rs` | Connection handling and login |
| `src/user.rs` | User records and bcrypt password hashes |
| `src/backend.rs` | Heartbeat and swap stubs |
| `src/addr_server.rs` | Async DNS name/address helper (MudOS `addr_server`) |
| `src/globals.rs` | Driver global state |

## Origin

Imported from `maadone74/maadOS-rust` `rust-src/` as created by earlier Cursor cloud agent work (PRs 1–5 on that repository). Runtime log files were not copied. The crate edition was set to 2021 so it builds on current stable Rust (the original `rust-src` listed edition 2024).
