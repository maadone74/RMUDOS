# rmudos agent instructions

rmudos is a **clean-room Rust LPC driver** inspired by [MudOS v22.2b14](https://github.com/lnsoso/mudos) (the lnsoso fork, including MySQL). It is **not** a line-by-line C translation.

Goal: implement MudOS-compatible LPC semantics in idiomatic Rust so mudlibs can run, while keeping the driver small, testable, and safe.

## Before changing code

1. Read `docs/ARCHITECTURE.md`, then the module you will edit.
2. If the task is “make this behave like MudOS”, consult the original C sources (clone `https://github.com/lnsoso/mudos` if needed) rather than guessing.
3. Prefer extending the existing layers over inventing parallel ones.

## Layer map

| Concern | Rust | MudOS C analogue |
| --- | --- | --- |
| CLI / config | `src/main.rs`, `src/config.rs` | `main.c`, `rc.c` |
| Game loop | `src/backend.rs` | `backend.c` |
| Telnet | `src/net/` | `comm.c`, `telnet.h` |
| Object lifecycle | `src/simulate.rs`, `src/vm/mod.rs` | `simulate.c`, `object.c` |
| Values | `src/vm/value.rs` | `svalue_t` in `lpc.h` |
| Bytecode / programs | `src/vm/program.rs` | `program.c`, `icode.c` |
| Interpreter | `src/vm/interpret.rs` | `interpret.c` |
| Applies | `src/vm/apply.rs` | `applies` |
| Compiler | `src/compiler/` | `lex.c`, `grammar.y.pre`, `generate.c`, `compiler.c` |
| Efuns | `src/efun/` | `efuns_main.c`, `func_spec.c`, `packages/` |
| Sample world | `mudlib/` | mudlib + `testsuite/` |

## Porting rules

- Preserve **observable LPC behavior** (efun signatures, applies, truthiness, inherit override, object paths).
- Rewrite **implementation**. No global `current_object` / `sp` / `csp`. Pass `Interpreter` and `MudWorld`.
- Do not port custom malloc, swap, LPC-to-C, Amiga/Windows ports, or debugmalloc.
- Missing applies are soft-noops (`LpcValue::Null`). Do not invent hard errors for optional hooks.
- Object paths: leading `/`, strip `.c`, stay inside the mudlib root.
- Heartbeat interval is 2 seconds (`HEARTBEAT_INTERVAL` 2000000 µs in MudOS).
- After driver or efun changes, update `docs/EFUNS.md` / `docs/LPC.md` / `docs/ARCHITECTURE.md` when behavior user-facing.

## Rust conventions

- `anyhow::Result` at driver boundaries; `bail!` for LPC runtime errors.
- Objects: `Arc<Mutex<Object>>` (`ObjectRef`). Never hold two object mutexes in an order that can deadlock; drop a guard before locking another object unless the current code already documents the order.
- Values: `LpcValue` is the dynamic type. Types in LPC source are mostly documentary.
- Async I/O stays in `backend` / `net`. The VM and compiler are synchronous.
- Register new efuns in `EfunTable::new` and document them in `docs/EFUNS.md`.
- Keep sample mudlib inside the compiler subset in `docs/LPC.md`.

## Verification

```bash
cargo test
cargo build --release
```

Add a focused unit test in `src/lib.rs` or next to the module for compiler/VM/efun work. Boot/connect regressions belong in `runtime_smoke`.

## Cursor extras

- Project rules: `.cursor/rules/`
- Skills: `.cursor/skills/` (porting, efuns, applies, compiler, tests)
- Subagents: `.cursor/agents/`
