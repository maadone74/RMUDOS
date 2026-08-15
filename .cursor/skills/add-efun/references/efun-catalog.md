# Efun catalog (rmudos vs MudOS func_spec)

## Implemented in `EfunTable`

write, say, tell_object, tell_room, message, debug_message,
capitalize, lower_case, strlen, explode, implode, member_array, sizeof, keys, values, sprintf, atoi, to_string, typeof,
clone_object, load_object, find_object, destruct, move_object, environment, all_inventory, file_name,
this_player, previous_object, users, call_other, time, shutdown.

Compiler intrinsic (not in the table): `this_object`.

## Common MudOS efuns not implemented (core `func_spec.c`)

**Eval / control:** call_out, remove_call_out, find_call_out, call_out_info, throw, error, set_eval_limit, eval_cost, origin, catch (language)

**Objects:** present, first_inventory, next_inventory, deep_inventory, children, objects, inherits, replace_program, reload_object, master, clonep, objectp, virtualp, shadow, query_shadowing, set_heart_beat, query_heart_beat, set_hide, set_reset

**Player / action:** this_interactive, this_user, input_to, get_char, add_action, query_verb, command, living, livings, find_living, find_player, notify_fail, enable_commands, userp, interactive, query_idle, exec, snoop, receive, shout, printf

**Types:** intp, floatp, stringp, pointerp/arrayp, mapp, undefinedp, functionp, bufferp, classp, to_int, to_float

**Strings / arrays / maps:** replace_string, strsrch, strcmp, allocate, allocate_mapping, map_delete, sort_array, filter, map, unique_array, regexp, implode-with-function, sprintf remaining directives

**Files:** save_object, restore_object, save_variable, restore_variable, read_file, write_file, read_bytes, write_bytes, get_dir, file_size, rm, rmdir, mkdir, cp, rename, stat

**Misc:** random, ctime, localtime, crypt, uptime, memory_info, get_config, resolve, query_ip_number, query_ip_name, flush_messages, function_exists, previous_object(-1) all previous

## Packages (lnsoso tree)

| Package | Spec | Notes |
| --- | --- | --- |
| db | `packages/db_spec.c` | MySQL default (`USE_MYSQL`) — later optional feature |
| sockets | `packages/sockets_spec.c` | LPC sockets |
| uids | `packages/uids_spec.c` | uid/euid |
| math | `packages/math_spec.c` | math efuns |
| contrib | `packages/contrib_spec.c` | extras |
| develop | `packages/develop_spec.c` | wiz/dev |
| parser | `packages/parser_spec.c` | parse_command (off in options.h) |
| qqwry | `packages/qqwry_spec.c` | IP location; skip unless requested |

When adding from this list, match default arguments and aliases (`this_user` → `this_player`, `arrayp` → `pointerp`, etc.) or document a smaller rmudos surface if the alias is unused.
