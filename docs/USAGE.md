# rmudos — Usage Guide

**rmudos** is a Rust LPC driver inspired by MudOS / FluffOS. It is a clean-room redesign, not a line-by-line C++ port. The driver compiles LPC objects from a **mudlib** directory into bytecode, runs them on a stack VM, and exposes telnet players to that world.

This document covers building, configuring, and running the driver. For authoring LPC worlds, see [MUDLIB.md](MUDLIB.md). For a full efun reference, see [EFUNS.md](EFUNS.md).

---

## What you get

| Layer | Role | Source |
| --- | --- | --- |
| Backend / game loop | Boot master, accept sockets, heartbeats | `src/backend.rs` |
| Networking | Line-oriented telnet I/O | `src/net/` |
| Simulate | Master boot, player connect/disconnect | `src/simulate.rs` |
| VM | Objects, applies, interpreter | `src/vm/` |
| Compiler | Lex → parse → bytecode | `src/compiler/` |
| Efuns | Built-in LPC functions | `src/efun/` |
| Mudlib | Your game world (LPC `.c` files) | `mudlib/` |

Runtime flow:

```
config.toml
    → MudWorld
    → load master (/secure/master)
    → apply create() then preload()
    → TCP listen
    → on connect: master->connect() → clone user → logon()
    → each line: user->process_input(line)
    → every 2s: heart_beat() on objects that define it
```

---

## Requirements

- **Rust 1.75+** via [rustup](https://rustup.rs/)
- Works on Linux, Windows, and macOS
- A telnet client (or any raw TCP line client) to connect

---

## Build

From the `RMUDOS` package directory (where `Cargo.toml` lives):

```bash
cd RMUDOS
cargo build --release
```

Binary path:

- Unix: `./target/release/rmudos`
- Windows: `.\target\release\rmudos.exe`

Debug build (faster compile, slower runtime):

```bash
cargo build
```

Run the included smoke tests (compile all sample mudlib objects and boot master):

```bash
cargo test
```

---

## Run

Minimal:

```bash
cargo run --release -- --config config.toml
```

Or with the release binary:

```bash
./target/release/rmudos --config config.toml
```

Common overrides:

```bash
rmudos --config config.toml --port 4000
rmudos --config config.toml --mudlib /path/to/my_mudlib
```

### CLI flags

| Flag | Meaning |
| --- | --- |
| `-c`, `--config PATH` | Load driver settings from a TOML-like config file |
| `-p`, `--port N` | Override `port` from the config |
| `--mudlib DIR` | Override mudlib root directory |
| `-h`, `--help` | Print usage |

If `--config` is omitted, defaults are used (`mud_name=RustMud`, `bind=0.0.0.0`, `port=4000`, `mudlib=mudlib`, `master=/secure/master`).

### Logging

Uses the `tracing` crate. Default filter includes `rmudos=info`. Raise verbosity:

```bash
# Unix
RUST_LOG=rmudos=debug cargo run --release -- --config config.toml

# Windows PowerShell
$env:RUST_LOG="rmudos=debug"; cargo run --release -- --config config.toml
```

Mudlib `debug_message()` output appears under the `mudlib` tracing target.

---

## Connect

```bash
telnet 127.0.0.1 4000
```

With the bundled sample mudlib you should see a welcome banner and land in **The Void**. Sample commands:

| Command | Action |
| --- | --- |
| `look` / `l` | Describe current room |
| `go <dir>` / `north` … | Move through exits |
| `say <text>` | Speak to others in the room |
| `who` | List interactive players |
| `help` | Command summary |
| `quit` | Disconnect |

Returning `0` from `process_input` ends the session (the sample `quit` command does this).

---

## Configuration (`config.toml`)

Example shipped with the project:

```toml
mud_name = "RustMud"
bind = "0.0.0.0"
port = 4000
mudlib = "mudlib"
master = "/secure/master"
max_cost = 1000000
```

| Key | Type | Default | Description |
| --- | --- | --- | --- |
| `mud_name` | string | `RustMud` | Display / log name for this mud |
| `bind` | string | `0.0.0.0` | Interface to bind the telnet listener |
| `port` | integer | `4000` | TCP port |
| `mudlib` | path | `mudlib` | Root directory of LPC sources. Relative paths are resolved against the config file’s directory |
| `master` | object path | `/secure/master` | Object path of the master (leading `/` optional; `.c` suffix stripped) |
| `max_cost` | integer | `1000000` | Instruction budget per LPC apply (must be &gt; 0) |

Notes:

- Lines may use `#` comments (outside quoted strings).
- Values may be bare or double-quoted; escapes `\n`, `\r`, `\t`, `\"`, `\\` work inside quotes.
- Object paths always normalize to `/foo/bar` form (no `.c`).

---

## Directory layout

Typical package layout:

```
RMUDOS/
├── Cargo.toml
├── config.toml
├── README.md
├── docs/                 ← this documentation
├── mudlib/               ← LPC world (game content)
│   ├── secure/
│   │   └── master.c      ← required entry point
│   ├── std/
│   │   ├── room.c
│   │   └── user.c
│   └── room/
│       ├── void.c
│       ├── tavern.c
│       └── street.c
└── src/                  ← Rust driver
```

On disk, object `/secure/master` maps to file `mudlib/secure/master.c`. Path traversal (`..`, absolute escapes) is rejected when resolving inherits and object files.

---

## Boot sequence (driver contract)

Understanding this contract is required to write a working mudlib.

1. **Load master** — `load_object(master)` compiles and instantiates the master blueprint, then calls `create()`.
2. **Preload** — driver applies `preload()` on master (optional; missing apply is a soft no-op). Use this to `load_object()` rooms and other singletons.
3. **Listen** — TCP accept loop starts.
4. **Connect** — for each socket:
   - Driver applies `master->connect()`.
   - Expected return: an **object** (normally `clone_object("/std/user")`).
   - If the return is not an object, the driver falls back to cloning `/std/user`.
   - The object is marked interactive and `logon()` is applied with `this_player` set to that object.
5. **Input loop** — each line applies `process_input(string line)` on the player.
   - Return `0` → disconnect and destruct the player object.
   - Any other return → keep the connection.
6. **Heartbeat** — every **2 seconds**, every live object that defines `heart_beat()` gets that apply.
7. **Shutdown** — mudlib may call efun `shutdown()`; the accept loop exits when shutdown is requested.

---

## Objects: blueprints vs clones

| Operation | Behavior |
| --- | --- |
| `load_object(path)` | Compile if needed, create **one** named blueprint instance, call `create()`, register under that path. Subsequent loads return the same object. |
| `clone_object(path)` | Compile, create a **new** instance with a clone number (`file_name` like `/std/user#3`), call `create()`. Not registered as the blueprint. |
| `find_object(path)` | Look up existing blueprint only; returns `0` if missing. |
| `destruct(obj)` | Mark destroyed, detach inventory/environment, remove blueprint registration if it was the blueprint. |

Players should almost always be **clones**. Rooms and master are usually **loaded** (singletons).

Containment:

- `move_object(dest)` or `move_object(obj, dest)` updates environment/inventory and applies `init()` on both destination and mover.
- `environment()`, `all_inventory()`, `file_name()` inspect the graph.

---

## Driver applies (hooks)

Missing applies are soft no-ops (return null) unless the driver explicitly requires a value.

| Apply | Who | When | Notes |
| --- | --- | --- | --- |
| `create()` | every object | after load/clone | Initialize globals |
| `preload()` | master | after master boot | Load the world |
| `connect()` | master | new TCP client | Return player object |
| `logon()` | player | after interactive attached | Welcome + starting room |
| `process_input(string)` | player | each input line | Return `0` to quit |
| `init()` | object / container | after `move_object` | Optional setup |
| `heart_beat()` | any defining object | every 2s | Periodic work |
| `reset()` | (mudlib convention) | not auto-called yet | Present in sample rooms only |

Named apply constants also exist in Rust (`create`, `connect`, `process_input`, `heart_beat`, `get_preload`, `clean_up`); only the ones wired in `backend` / `simulate` / `MudWorld` are invoked automatically today.

---

## Switching mudlibs

Point the driver at another root:

```toml
mudlib = "../my_world"
master = "/secure/master"
```

Or:

```bash
rmudos --config config.toml --mudlib C:\games\my_mudlib
```

The new root must contain a compilable master object at the configured path (default `secure/master.c`).

---

## Troubleshooting

| Symptom | Likely cause |
| --- | --- |
| Fail at startup on master | Missing `mudlib/secure/master.c`, bad path, or LPC compile error |
| Connect but no welcome | `connect()` / `logon()` missing or erroring; check logs |
| `Unknown command` only | Your user object’s `process_input` is incomplete |
| Compile error on inherit | Cyclic inherit, bad path, or escaped mudlib root |
| `max_cost` errors | Infinite loop or too much work in one apply |
| Port in use | Change `port` or stop the other listener |

Compile-check the sample mudlib without running the server:

```bash
cargo test compile_mudlib -- --nocapture
```

---

## Related docs

- [MUDLIB.md](MUDLIB.md) — create and structure LPC mudlibs
- [EFUNS.md](EFUNS.md) — built-in function reference
- [LPC.md](LPC.md) — language subset supported by this driver
- [ARCHITECTURE.md](ARCHITECTURE.md) — driver internals for contributors
