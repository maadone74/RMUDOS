# What to port next vs skip

## High value (mudlib-facing)

1. More core efuns used by typical libs: `present`, `first_inventory`/`next_inventory`, `deep_inventory`, `random`, `input_to`, `call_out`/`remove_call_out`, `query_idle`, `userp`/`interactive`, `master`, `children`, `inherits`, `explode`/`replace_string`/`strsrch` (partial explode exists).
2. File efuns: `read_file`, `write_file`, `file_size`, `get_dir`, `rm`, `mkdir` — gated later by `valid_read`/`valid_write`.
3. `save_object` / `restore_object` (`.o` files, `SAVE_EXTENSION`).
4. `catch` / `throw` / `error` in compiler + interpreter.
5. Applies: `reset`, `catch_tell`, `net_dead`, `write_prompt`, `clean_up`, master `valid_*`, `compile_object` (virtual objects), `crash`, `epilog`.
6. Preprocessor or a tiny `#include` so real mudlibs can share headers from MudOS `include/`.
7. `call_out` wheel (`call_out.c`, `CALLOUT_HANDLES`, `THIS_PLAYER_IN_CALL_OUT`).

## Medium

- simul_efun object
- UID package (`packages/uids.c`) if targeting that mudlib style
- `add_action` / living hash **or** keep command parsing in mudlib `process_input` (current sample)
- function pointers / `bind` / `evaluate`
- mixed-key mappings, buffers
- `foreach`, `switch`, `for`
- LPC sockets (`PACKAGE_SOCKETS`)
- MySQL (`PACKAGE_DB` in this fork) as an optional Cargo feature

## Skip unless explicitly requested

Custom malloc, swap, LPC-to-C, binaries, `ed`, Amiga/Win32, `addr_server` as required, opcode profiling, `debugmalloc`, `qqwry` IP geolocation package, matrix package.

## Intentional rmudos differences (keep unless changing policy)

- Tokio tasks per connection instead of a single `select` backend
- Config file `config.toml` instead of MudOS runtime config + `options.h` compile macros
- Small curated efun table
- No privilege model yet
- Mapping keys are strings
