# Driver map and known MudOS gaps

## Where to edit

| Concern | Files |
| --- | --- |
| Applies, commands, move/init | `RMUDOS/src/vm/mod.rs` |
| Interpreter, simul vs efun | `RMUDOS/src/vm/interpret.rs` |
| Object, `Action`, interactive | `RMUDOS/src/vm/object.rs` |
| Core efuns | `RMUDOS/src/efun/mod.rs` |
| MudOS extras (`add_action`, `exec`, sockets) | `RMUDOS/src/efun/mudos_extra.rs` |
| Telnet / heartbeats | `RMUDOS/src/backend.rs`, `RMUDOS/src/net/` |
| Nightmare login/user/room | `RMUDOS/mudlib/adm/obj/login.c`, `std/user.c`, `std/living.c`, `std/room/exits.c` |

Boot: load `simul_efun` then master (`config.toml`). Connect: `master->connect()` → login object → `exec` into `/std/user` → `setup()`.

## Known playable-path fixes (do not regress)

- Interactive `write`/`command` after `exec`
- `input_to` on interactive; restore pending on callback error
- MudOS `sprintf` pad/center forms
- simul preferred over efun; `efun::` bypasses
- `origin()` for `force_me` / alias reset
- Socket efuns stub `EESOCKET`
- `random`, `sin`/`cos` and related math for creation/heartbeats
- simul `destruct`/`exec` permissions for `/adm/obj/login` → `/std/user`
- `crypt` 2-char salt
- Skip `CMD_D` in `cmd_hook` when `query("in creation")`
- No on-demand `CMD_D` `get_dir` rehash
- No `SOUL_D` fallback in `cmd_hook` (first load hangs)
- Castle room does not clone `/std/bboard` in `create()`
- `(string)` of missing `affect_environment` must not append `0` to room long desc

## Docs

`RMUDOS/docs/USAGE.md`, `ARCHITECTURE.md`, `LPC.md`, `EFUNS.md`, `MUDLIB.md`.
