---
name: write-driver-tests
description: Add rmudos compiler, VM, efun, boot, or mudlib tests. Use when verifying a port, fixing a runtime bug, or extending smoke tests.
---

# Write driver tests

Existing smoke tests live in `src/lib.rs`:

- `compile_mudlib` — every sample object path compiles
- `boot_master_only` — master boot loads `/room/void`
- `connect_sends_welcome` — fake interactive gets welcome/void text

Run: `cargo test`

## Patterns

**Compiler:** `compiler::compile_source(src, "test")` for isolated snippets; `compile_file_in(&mudlib, "/path")` for inherit.

**Runtime:** build `MudWorld::new(DriverConfig { mudlib, ..Default::default() })`, `simulate::boot_master`, then `world.apply` / efuns / `clone_object`.

**Connect:** unbounded `mpsc` like `connect_sends_welcome`; drain with `try_recv`.

**LPC-in-test:** prefer a string compiled in-memory when you do not need inherit. For applies that need a path, add a fixture under `mudlib/` only if it is useful as sample world; otherwise keep tests in Rust.

## What to assert

- MudOS truthiness and `0` vs `"0"`
- Destructed objects are falsy and not in `find_object`
- Missing apply returns null
- Inherit override
- Efun arity errors include the name
- `process_input` `0` disconnect contract if you touch backend

Do not require a live TCP port for unit tests. `cargo test` must stay offline.

If you add language features, extend `compile_mudlib` paths or add a dedicated test so the sample world cannot silently drop coverage.
