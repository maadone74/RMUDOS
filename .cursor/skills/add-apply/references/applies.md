# MudOS applies (`applies` file)

Format: `TOKEN` or `TOKEN:lpc_name`. Driver uses the LPC name (after colon if present).

## Object applies

| LPC function | When (MudOS) | rmudos |
| --- | --- | --- |
| `__INIT` (`#global_init#`) | program init | global initializers in codegen / `create` |
| `create` | after load/clone | yes, on load/clone |
| `init` | after move into environment | yes, dest + mover |
| `reset` | periodic reset | no |
| `clean_up` | idle reclaim | enum exists; not scheduled |
| `heart_beat` | if enabled / function present | yes, 2s if function exists |
| `logon` | new interactive | yes |
| `process_input` | player line | yes |
| `catch_tell` | text to NPC/listener | no |
| `net_dead` | disconnect | no |
| `write_prompt` | prompt | no |
| `id` | identify | no |
| `move_or_destruct` | special move | no |
| `receive_message` | `message()` classed | `message` efun does not call it |
| `telnet_suboption`, `terminal_type`, `window_size` | telnet | no |
| parse_command id lists | parser package | no |

## Master applies

| LPC function | Role | rmudos |
| --- | --- | --- |
| `connect` | return user object | yes |
| `preload` | boot world | yes (no numbered preload loop / `epilog`) |
| `epilog` | after preload | no |
| `compile_object` | virtual objects | no |
| `valid_read` / `valid_write` / `valid_link` / `valid_socket` / `valid_save_binary` / `valid_shadow` / `valid_object` / `valid_override` / `valid_seteuid` / `valid_bind` / `valid_hide` / `valid_database` / `valid_asm` / `valid_compile_to_c` | security | no |
| `creator_file` / `domain_file` / `author_file` / `privs_file` | uids/privs | no |
| `get_root_uid` / `get_bb_uid` | uids | no |
| `error_handler` / `log_error` / `crash` / `slow_shutdown` | errors | tracing only |
| `make_path_absolute` | path policy | normalize in Rust |
| `object_name` | display name | no |
| `flag` | command-line flags | no |

When implementing preload like MudOS, master may return an array of paths from `epilog`/`preload`; rmudos currently calls `preload()` with no arguments and ignores the return value. Changing that is a compat project — do not assume numbered `preload(n)` unless you implement it.
