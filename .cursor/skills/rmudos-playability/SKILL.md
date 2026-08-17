---
name: rmudos-playability
description: Diagnose RMUDOS telnet hangs, failed movement, character creation, and "You cannot go that way." Use when the user reports stuck input, bad input freezing the screen, Newbieville exits, login/exec, or OneDrive filesystem issues.
---

# RMUDOS playability debug

Playable path: telnet → name/password/email → race/stats (`setter.c`) → `ROOM_NEWBIE` (`/d/newbieville/rooms/townsquare`).

## Classify the failure

**Hang (no prompt, no error)**  
Almost always blocking FS or first-load of a heavy object, not a deadlock in the VM.

Check in order:
1. `cmd_hook` → `CMD_D->find_cmd` → `rehash`/`get_dir` (unknown verbs like `outh`)
2. `SOUL_D` / `CHAT_D` / `BBOARD_D` / news on first command or room enter
3. `find_object_or_load` of an exit dest whose `create()` clones boards or scans dirs
4. Guild `get_dir` during race `pick()`

Fix: skip or `catch` the load; return `0` or a soft message. Never add new `get_dir` on the command path.

**"You cannot go that way."** with listed exits  
`use_stupid_exit` won. Causes: FIFO dispatch, stub registered for real verbs, or `add_action` stored on the room instead of the player. Driver must bind sentences to the living, LIFO, `use_exit` for real dirs only.

**Movement message then freeze**  
Dest room `create()`/`init()` is hanging. Soft-load dest; keep the player in place with "not available yet."

**Creation password / duplicate email**  
`crypt` salt mismatch, or simul `destruct` blocked for unfinished `/std/user`.

**Looks/desc ends with `0`**  
`(string)` cast of `0` from missing `affect_environment`. Guard with `stringp()`.

## Confirm the running binary

Mudlib `.c` changes apply after reload/restart. Driver `.rs` changes need **kill + new release binary**. Stale `rmudos` is a common false "fix didn't work."

Linux/local disk helps FS hangs only. Semantic bugs reproduce everywhere.

## Newchar / rooms to treat carefully

- `mudlib/d/standard/setter.c` — creation; avoid guild dir scans
- `mudlib/std/living.c` — `cmd_hook`
- `mudlib/daemon/command.c` — no on-demand `get_dir` rehash
- `mudlib/std/room/exits.c` — `initiate_exits` / `use_exit`
- `mudlib/d/newbieville/rooms/*` — do not spawn boards in `create()`

## Tests

Prefer isolated `src/lib.rs` tests. Do not add automated pick/newchar tests that `get_dir` the whole mudlib on this workspace.
