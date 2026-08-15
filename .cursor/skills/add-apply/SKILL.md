---
name: add-apply
description: Add a MudOS driver apply (create, logon, process_input, valid_read, catch_tell, reset, …). Use when wiring driver callbacks into LPC objects or master.
---

# Add a driver apply

MudOS lists applies in the `applies` file. The driver looks up a function by name (or the renamed form after the colon) and calls it with `ORIGIN_DRIVER`.

rmudos uses string names via `MudWorld::apply` and a small `ApplyName` enum in `src/vm/apply.rs`. Missing functions return `Null`.

## Steps

1. Read [references/applies.md](references/applies.md) for the C name, LPC name, arguments, and when the driver fires it.
2. Add a variant to `ApplyName` if the driver will call it in Rust (not required for mudlib-only hooks).
3. Call `world.apply(object, "name", args, this_player, previous_object)` from backend/simulate/efun/VM — not from random new globals.
4. Attach interactives **before** `logon` so welcome `write`s flush (existing connect path).
5. `process_input` returning falsey `0` still means drop the connection (`backend.rs`).
6. Optional hooks (`reset`, `catch_tell`, `net_dead`) must tolerate missing functions.
7. Master security applies (`valid_read`, `valid_write`, …) default to “allow” only if you document that; do not silently skip once file efuns exist — implement the apply or refuse the operation.
8. Test with a tiny mudlib object that defines the apply, and one that does not.
9. Mention the apply in `docs/USAGE.md` or `docs/MUDLIB.md` if authors must implement it.

## Do not

- Error when an optional apply is absent.
- Hold the object mutex across `apply`.
- Invent apply names that are not in MudOS unless it is clearly rmudos-only and documented.
