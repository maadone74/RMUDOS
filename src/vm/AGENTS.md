# VM agents

`MudWorld` is the process: objects, blueprints, master, efuns, ids, shutdown.

- Applies: missing function returns `Null`. Do not error optional hooks.
- Never apply while holding the target `Mutex`.
- `LpcValue::Mapping` is string-keyed for now; mixed mapping keys are a later MudOS-compat task.
- Heartbeat only if `program.has_function("heart_beat")`.
- MudOS refs: `interpret.c`, `object.c`, `lpc.h`, `simulate.c`, `applies`.
