---
name: rmudos-mudlib-patch
description: Change Nightmare/Darke LPC in RMUDOS/mudlib safely. Use when editing .c/.h under mudlib, room exits, login, living cmd_hook, daemons, or when a driver-compatible mudlib workaround is needed.
---

# Patching the Nightmare mudlib

Keep diffs small and Nightmare-shaped. Do not rewrite files to "modern C" or add FluffOS-only syntax unless the driver already compiles it (`RMUDOS/docs/LPC.md`).

## Allowed mudlib patches

- Skip FS-heavy daemons on this driver (`SOUL_D`, `BBOARD_D`, `CMD_D` rehash)
- `catch` around first-load `call_other` / `find_object_or_load`
- Soft-fail dest rooms in `use_exit`
- `in creation` guard in `cmd_hook`
- Fix lib bugs (missing `return`, bad `(string)` casts)

Do **not** paper over driver bugs (wrong `this_player`, `add_action` on the room, FIFO actions) in LPC. Fix `src/` instead.

## Exits

`initiate_exits` (called from room `init()` every enter):

1. `clear_actions()` so re-enter does not stack stubs
2. `add_action("use_exit", dir)` for each real exit + short form (`n`/`s`/…)
3. `use_stupid_exit` only for compass dirs that are **not** exits

`use_exit` uses `query_verb()`, expands shorts, `catch` loads dest.

## Commands

Unknown verbs must return `0` quickly (`What?`), not `get_dir` the cmds tree.

## Style

- Inherit `ROOM` / `std.h`; `add_exit(path, dir)` in `create()`
- Action lfuns: `int foo(string arg)` → `1` handled / `0` fail
- No new `printf` for player text unless the surrounding file already uses it
