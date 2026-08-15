---
name: add-efun
description: Add or extend a MudOS-compatible LPC efun in rmudos. Use when implementing a built-in, registering EfunTable, matching func_spec.c, or documenting efuns.
---

# Add an efun

MudOS prototypes: `func_spec.c` and `packages/<name>_spec.c`. Bodies: `efuns_main.c`, `efuns_port.c`, package `.c` files.

rmudos: implement in `src/efun/mod.rs` (or a new submodule), register in `EfunTable::new`, document `docs/EFUNS.md`.

## Steps

1. Look up the prototype and aliases (e.g. `load_object` is `find_object` with default 1 in MudOS; rmudos has separate functions — match **mudlib-visible** names used here).
2. Check [references/efun-catalog.md](references/efun-catalog.md) so you do not duplicate or skip a dependency efun.
3. Signature: `fn(&mut Interpreter<'_>, Vec<LpcValue>) -> Result<LpcValue>`.
4. Validate arity with existing `require`; mention the efun name in errors; argument numbers are 1-based.
5. Resolve objects through interpreter helpers (`resolve_object`, `object_argument`) so paths load consistently.
6. Register: `functions.insert("name", name);`
7. If the compiler must treat it as intrinsic (`this_object`), do **not** register it; add an `Op` instead.
8. Test via a compiled LPC function or direct call. Cover wrong types, missing objects, extra optional args.
9. Update `docs/EFUNS.md`.

## Packages

Do not fold MySQL/sockets/uids into the core table. New file `src/efun/<package>.rs` and a clear feature/module boundary.

## simul_efun

There is no simul_efun object. Do not silently implement a missing efun as LPC in the driver. Either add the efun or write a mudlib helper the compiler can actually call.
