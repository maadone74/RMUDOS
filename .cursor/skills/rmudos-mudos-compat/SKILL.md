---
name: rmudos-mudos-compat
description: Implement or fix MudOS-compatible driver behavior (add_action, this_player after exec, input_to, simul_efun, origin, crypt, sprintf). Use when changing RMUDOS Rust efuns/VM/applies, or when LPC commands hit the wrong object or ignore room verbs.
---

# RMUDOS MudOS compatibility

Match MudOS LPC runtime behavior in the Rust driver. Do not port C++ MudOS source.

## Decide driver vs mudlib

| Symptom | Likely layer |
| --- | --- |
| Verb exists but wrong handler / "You cannot go that way" | `add_action` owner/giver, LIFO, `query_verb` |
| Writes go to login after `exec` | interactive preference in `write`/`command` |
| `input_to` callback on room / lost prompt | pending on interactive; restore on error |
| `force_me` / aliases broken | missing `origin()` |
| Password verify fails on new chars | `crypt()` 2-char salt |
| Same-named efun vs simul | simul wins unless `efun::` |

Patch mudlib only if the lib is wrong or a FS workaround is required. Fix the efun/apply when Nightmare code is correct and the driver is not.

## `add_action` (required shape)

1. Store `Action { verb, fun, catch_all, owner }` on the **command giver**.
2. `owner` = `current_object`. Giver = `this_player`, except: if `current_object` has `commands_enabled` and is not `this_player` (post-`exec` `user::setup`), giver = owner.
3. Dispatch only the living's sentence list, **newest first**. Apply `fun` on `owner` with `this_player` = the living.
4. On `move_object`, drop sentences whose owner is not nearby; then `init()` dest, dest inventory, mover (`this_player` = mover).
5. `clear_actions` removes sentences this object registered on the giver, not "clear the room's vec".

Exact-verb pass first, then catch-all. Bare verb → no LPC argument.

## Tests

Add a small `src/lib.rs` unit test that compiles LPC, `enable_commands`, `move_object`, `handle_player_input`. See `room_add_action_binds_to_this_player` and `catch_all_add_action_and_process_input`.

Do not rely on full newchar telnet tests for efun semantics (OneDrive `get_dir` hangs).

## After code change

`cargo build --release` in `RMUDOS/`, kill old `rmudos`, start the new binary. LPC mudlib edits need object reload or restart.

## Extra reference

- Driver map: [reference.md](reference.md)
