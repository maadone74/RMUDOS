# Architecture Notes

Concise map of the Rust driver for contributors reviewing or extending rmudos.

---

## Crate layout

| Module | Responsibility |
| --- | --- |
| `main.rs` | CLI, logging, build `MudWorld`, run backend |
| `config.rs` | `DriverConfig` parse / defaults / path normalize |
| `backend.rs` | Boot, TCP accept, input → `process_input`, heartbeat task |
| `net/telnet.rs` | Async line I/O, basic Telnet IAC strip, CRLF out |
| `simulate.rs` | `boot_master`, `connect_player`, `destruct_object` |
| `compiler/` | Lexer, parser, inherit resolution, bytecode codegen |
| `vm/` | `MudWorld`, objects, values, interpreter, apply names |
| `efun/` | Built-in LPC functions |

---

## Object runtime

`MudWorld` owns:

- `objects`: id → `ObjectRef` (`Arc<Mutex<Object>>`)
- `blueprints`: normalized path → object id (singletons from `load_object`)
- `master` handle
- `EfunTable`
- shutdown flag, id allocators

`Object` fields: program, globals, environment (weak), inventory, optional `Interactive` (socket output + peer), clone number, destructed flag.

Applies go through `MudWorld::apply` → `Interpreter`. Missing functions return null without error (optional hooks).

---

## Compiler

`compile_file_in(mudlib_root, "/path")`:

1. Guard cycles with a visiting set
2. Parse inherits; compile them recursively
3. Merge inherited globals/functions; compile local functions (overrides win)
4. Ensure `create` exists (synthesize empty if needed after inherit merge rules in codegen)

Array literals `({ ... })` are rewritten toward `[ ... ]` before parse for grammar convenience.

---

## Networking model

- One Tokio task per connection.
- Unbounded channels carry outbound text and inbound lines.
- Interactive attachment happens **before** `logon()` so welcome text can flush.
- On `process_input` → `0`, interactive is cleared and the object is destructed.

Heartbeats run on a separate 2s interval task over objects with `set_heart_beat` enabled. The same tick processes due `call_out` entries; every ~60s a coarse `reset()` sweep runs.

---

## Design stance vs MudOS

Intentional similarities: master object, applies, load/clone, efun names, mudlib path mapping, telnet players.

Intentional differences / current gaps:

- Clean-room Rust, not a C++ port
- Growing MudOS efun set (filesystem sandbox, commands, login helpers)
- simul_efun object not yet auto-loaded
- Preprocessor is real but `#if` / function-like macros are limited
- `catch_tell` not specially wired beyond interactive `write`
- Config is a simple key=value file, not MudOS `config.h` macros

Boot: prefer master `epilog(0)` → preload each path; if `epilog` is absent, call void `preload()` (used by `/secure/master`).

`config.toml` may set `master = "/adm/obj/master"` and `simul_efun = "/adm/obj/simul_efun"`. Simul_efun is loaded before master; unknown efun names fall through to that object. Keep `/secure/*` as a lightweight regression harness (default in unit tests).

When reviewing gameplay bugs, check the mudlib first. When reviewing language/runtime bugs, start at `compiler/` and `vm/interpret.rs`.

---

## Tests

In `src/lib.rs`:

- `compile_mudlib` — compiles every sample object path
- `boot_master_only` — boots master and expects `/room/void`
- `connect_sends_welcome` — connects a fake interactive and expects welcome/void text

Run with `cargo test`.
