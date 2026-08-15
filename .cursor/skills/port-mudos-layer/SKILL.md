---
name: port-mudos-layer
description: Port a MudOS v22.2b14 C subsystem into rmudos Rust. Use when the user asks to port, reimplement, or match MudOS/FluffOS driver behavior (compiler, VM, efuns, applies, backend, packages).
---

# Port a MudOS layer into rmudos

Original tree: https://github.com/lnsoso/mudos (v22.2b14 + MySQL). Clone if the C sources are not already on disk (`git clone --depth 1 https://github.com/lnsoso/mudos.git /tmp/mudos`).

This repo is a **clean-room Rust driver**. Copy semantics, not C control flow, malloc, or globals.

## Workflow

1. **Name the slice.** One efun, one apply, one grammar feature, or one backend behavior. Do not port a 2000-line `.c` file in one change.
2. **Read C with a purpose.** Open the file from [references/file-map.md](references/file-map.md). Note `options.h` flags and `#ifdef` (this fork: `PACKAGE_DB`/`USE_MYSQL`, `PACKAGE_SOCKETS`, `PACKAGE_UIDS`, `NO_LIGHT`, `CALLOUT_HANDLES`, heartbeat 2s).
3. **Map to Rust.** Put the code in the module from the file map. Extend types (`LpcValue`, `Op`, `ApplyName`, `EfunTable`) instead of parallel worlds.
4. **Rust shape.**
   - Context: `&mut Interpreter` or `&MudWorld`, never thread-locals mimicking `current_object`.
   - Errors: `anyhow` with efun/object path.
   - Objects: `ObjectRef`; drop mutex guards before nested applies.
5. **Tests.** Compile LPC and/or boot `MudWorld` as in `src/lib.rs`. Cover destructed objects, missing applies, `0` vs empty string.
6. **Docs.** `docs/LPC.md`, `docs/EFUNS.md`, or `docs/ARCHITECTURE.md` as appropriate.
7. **Verify.** `cargo test`.

## Priority and skip lists

See [references/port-priority.md](references/port-priority.md).

## Done when

- Observable LPC behavior matches the C notes you captured (or documented intentional difference).
- No new global interpreter state.
- Tests and docs updated.
