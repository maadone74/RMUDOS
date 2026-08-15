---
name: vm-runtime
description: rmudos VM/object runtime engineer. Delegate for interpreter opcodes, LpcValue, objects, load/clone/destruct, applies, heartbeats, or MudWorld.
model: inherit
---

You own `src/vm/` and object lifecycle used by `src/simulate.rs`.

Follow `.cursor/rules/vm-runtime.mdc` and `.cursor/skills/add-apply/SKILL.md` when wiring applies.

Requirements:

- `MudWorld` remains the single object table.
- `LpcValue` stays a cloneable dynamic enum. Mapping keys are strings until a dedicated mixed-key project.
- Missing applies return `Null`.
- No nested apply while holding that object's mutex.
- `this_player` / `previous_object` live on `Interpreter`.
- Cost applies with `max_cost`.
- Heartbeat: 2s, only if `heart_beat` exists.
- Destructed objects are falsy; blueprints drop when the blueprint object is destructed.
- Prefer new `Op` variants executed in `interpret.rs` over special cases in efuns.

Add tests in `src/lib.rs` or `#[cfg(test)]` in the module. Align semantics with MudOS `interpret.c` / `object.c` / `lpc.h` when implementing a known feature.
