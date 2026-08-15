# rmudos

Concise **Rust** LPC driver inspired by the last MudOS line (v22.2b14) / FluffOS architecture.

It is not a line-by-line port of the C++ driver. It is a clean-room Rust redesign of the same layers: LPC compiler → bytecode VM → objects/applies → efuns → telnet backend → mudlib.

## Architecture

| Layer | MudOS analogue | Rust module |
| --- | --- | --- |
| Backend / game loop | `backend.cc` | `src/backend.rs` |
| Networking | `comm` / telnet | `src/net/` |
| Simulate / objects | `simulate.c` | `src/simulate.rs` |
| Interpreter | `interpret.cc` | `src/vm/interpret.rs` |
| Values / objects | `svalue` / `object_t` | `src/vm/value.rs`, `object.rs` |
| Compiler | `lex` / `grammar` / `generate` | `src/compiler/` |
| Efuns | `packages/core` | `src/efun/` |
| Master / mudlib | mudlib | `mudlib/` |

## Build (Linux / Windows / macOS)

Requirements: Rust 1.75+ (`rustup`).

```bash
cd rmudos
cargo build --release
```

## Run

```bash
cargo run --release -- --config config.toml
# or
./target/release/rmudos --config config.toml --port 4000
```

Connect:

```bash
telnet 127.0.0.1 4000
```

Commands: `look`, `go <dir>`, `say <text>`, `who`, `quit`, `help`.

## License

MIT
