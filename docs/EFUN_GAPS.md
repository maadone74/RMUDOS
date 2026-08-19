# Efun gap inventory (std + adm/obj)

Prioritized list of MudOS efuns that Nightmare LPC in `mudlib/std` and `mudlib/adm/obj` still needs, compared with this Rust driver.

Sources:

- MudOS v22.2b14 LPC surface: [lnsoso/mudos `func_spec.c`](https://github.com/lnsoso/mudos/blob/master/func_spec.c) (names and signatures, not C internals)
- Nightmare man pages: [`mudlib/doc/efun/`](../mudlib/doc/efun/)
- RMUDOS registrations: [`src/efun/mod.rs`](../src/efun/mod.rs), [`src/efun/mudos_extra.rs`](../src/efun/mudos_extra.rs)
- Simuls actually loaded: [`mudlib/adm/obj/simul_efun.c`](../mudlib/adm/obj/simul_efun.c)
- Scan: [`tools/efun_scan.py`](../tools/efun_scan.py) (re-run after driver/mudlib changes)

Compiler intrinsics (not efun table): `this_object()`, `catch()`.

Same-named simuls (`message`, `write`, `destruct`, `exec`, `base_name`, `file_exists`, `shout`, `first_inventory`, `next_inventory`, …) are **not** missing unless LPC uses `efun::` and that efun is absent.

Unknown names abort the apply (`unknown efun` in `src/vm/interpret.rs`).

---

## Classification

| Class | Meaning |
| --- | --- |
| present | Registered in Rust and used in this scope |
| stub | Registered, dummy return (sockets, `ed`) |
| incomplete | Present but not MudOS-shaped |
| missing-used | Called from scoped LPC (or `efun::` from a loaded simul) and not registered |
| missing-unused | In MudOS/man pages, not called in this scope |
| simul | Fully implemented in simul_efun without a missing `efun::` |

---

## P0 — playable path (`login`, `master`, `user`, `living`, `move`, `room`)

| Efun | Class | Notes |
| --- | --- | --- |
| `clonep` | **present** | [`src/efun/mod.rs`](../src/efun/mod.rs) — uses `Object.clone_number` |
| `set_hide` | **present** | [`mudos_extra.rs`](../src/efun/mudos_extra.rs) + `valid_hide`; `find_object_for` respects `hidden` / `can_hide` |
| `strcmp` | **present** | [`src/efun/mod.rs`](../src/efun/mod.rs) |
| `query_snoop` | **present** | Driver efun for simul `efun::query_snoop` |
| `snoop` | **present** | Driver efun; output forwarded via `Object::write` |
| `query_snooping` | **present** | Driver efun for simul |

### Pre-scan notes (P0)

| Name | Result |
| --- | --- |
| `shout` | **simul-complete** — not a driver gap |
| `first_inventory` / `next_inventory` | **simul-complete** |
| `virtualp` | **present** — always `0` until virtual `compile_object` exists |

---

## P1 — std core (combat, shops, user editor, guilds)

| Efun | Class | Notes |
| --- | --- | --- |
| `map_mapping` | **present** | [`mudos_extra.rs`](../src/efun/mudos_extra.rs); `map()` alias dispatches mappings |
| `inherits` | **present** | Walks `Program.inherit_programs` |
| `sort_array` | **present** | Compare callback via function name / functional |
| `stat` | **present** | File metadata array; `-1` delegates to `get_dir` |
| `ed` | **stub** | Returns `0` so mudlib simple editor runs; full MudOS ed not implemented |

---

## P2 — rest of `std/` (spells, skills, pets, HM tools)

| Efun | Class | Notes |
| --- | --- | --- |
| `deep_inherit_list` | **present** | Driver efun for simul `efun::deep_inherit_list` |
| `call_out_info` | **present** | [`src/vm/call_out.rs`](../src/vm/call_out.rs) `info()` |
| `floatp` | **present** | [`src/efun/mod.rs`](../src/efun/mod.rs) |
| `parse_command` | **present** | Word parser + out-params (like `sscanf`); covers pet `'verb' / 'verb' %s` and `%w`/`%d`/`%o`/`%i`/`%l` |
| `in_edit` / `in_input` | **present** | `pending_input` + `editing_file` on `Object` |

---

## Also registered (used outside std / adm/obj, or commonly needed)

| Efun | Notes |
| --- | --- |
| `filter_mapping` | `filter()` now dispatches mappings |
| `unique_array` | Groups by callback / object apply |
| `children` / `objects` / `livings` | Object-set queries |
| `uptime` / `localtime` | Boot elapsed seconds; UTC civil array (`LT_*`) |
| `arrayp` | Alias of `pointerp` |
| `this_interactive` | Interactive `this_player`, else interactive `this_object` |
| `remove_action` | Drops matching sentence this object registered on the giver |
| `get_dir` / `file_size` | MudOS `-1`/`-2` / `get_dir(path, -1)` triples; 5s stat cache |
| `repeat_string` | `str` repeated `n` times (capped) |
| `rmdir` | Empty directory only (`remove_dir`) |
| `tail` | Last 20 lines of a file, written to the player |
| `inherit_list` | Immediate inherits (not deep) |
| `read_bytes` | Byte slice of a file as a string |
| `rusage` | Mapping of process times (`utime`/`usertime` milliseconds) |
| `debug_info` | Object flags (0) / program sizes (1), written to the caller |
| `dumpallobj` | Object table written to a mudlib file |
| `dump_file_descriptors` | Open fd list |
| `cache_stats` | FS cache hit rate |
| `malloc_status` / `mud_status` | Status strings (Nightmare `more(explode(...))`) |
| `author_stats` / `domain_stats` | Per-author/domain maps via master applies |

---

## `cmds/` inventory (mortal first)

Compile of top-level `_*.c` in `mortal`, `hm`, `creator`, `system`, `adm`, `mentor`: **265 ok**. Test: `compile_cmd_dirs`.

Mortal `cmd_*` apply with simul_efun loaded: **no unknown efun**. Test: `mortal_cmds_apply_without_unknown_efun` (skips FS-heavy `help`/`news`/`mail`/`finger`/`faq`/`bug`/`idea`/`typo`/`praise`/`mudidea`/`biography`/`background` and destructive `suicide`).

### Mortal — efun-ready

All 93 top-level mortal commands **compile**. Runtime unknown-efun scan is clean once simul_efun is loaded (`syntax`, `arrange_string`, `format_page`, `effective_light` are simuls).

Still mudlib/FS limited (not missing efuns): `help`/`news`/`mail`/`finger` (daemons + `more`/`get_dir`), `suicide` (`rmdir` now present).

Compile fixes applied: `_colour` (missing wizard include → local `valid_colour`), `_news` (`EndText);` here-doc trailer).

### Wizard/system — driver internals (cmds compile; efuns present)

| Efun | Used by | Notes |
| --- | --- | --- |
| `author_stats` | `cmds/creator/_realms.c` | Live objects grouped via `master->author_file`; `"moves"` from `move_object` |
| `domain_stats` | `cmds/creator/_domains.c` | Same shape via `master->domain_file` |
| `cache_stats` | `cmds/adm/_cache.c` | Writes FS stat-cache hit/miss |
| `malloc_status` | `cmds/creator/_malloc.c` | Returns process `Vm*` text (Nightmare `more()` expects a string) |
| `mud_status` | `cmds/creator/_mstatus.c` | Returns driver summary string; extra flag adds tables |
| `debug_info` | `cmds/system/_debug_info.c` | Op 0 object flags, op 1 program stats |
| `dumpallobj` | `cmds/adm/_dumpallobj.c` | Writes object list to mudlib path (`/OBJ_DUMP` default) |
| `dump_file_descriptors` | `cmds/adm/_fdinfo.c` | `/proc/self/fd` table |
| `rusage` | `cmds/adm/_bench.c` | Mapping; `utime`/`stime` ms plus Nightmare `usertime` |

### Wizard — compile skip (not efuns)

| File | Why |
| --- | --- |
| `cmds/creator/_pupdate.c` | `CONFIG_DIR` never defined |
| `cmds/creator/_unref.c` | `inherit REFS_D` is a concatenated path, not a string literal |

---

## Incomplete / stub (already registered)

| Efun | Issue |
| --- | --- |
| Socket family | Stubs return `EESOCKET`. Not called from `std` / `adm/obj`. |
| `ed` | Stub — returns `0`; wizard full-screen ed not implemented. |
| `virtualp` | Always `0` (no virtual compile_object flag yet). |
| `sprintf` | Subset of MudOS formats. |
| `localtime` | UTC only (`LT_ZONE` = `"UTC"`, `LT_GMTOFF` = `0`). |

---

## Simul-complete (not a driver gap)

`shout`, `first_inventory`, `next_inventory`, `say`/`tell_object`/`tell_room` (message wrappers), `cat`, `extract`, `hiddenp` (uses `find_object` + euid; driver `set_hide` now backs real hide).

---

## missing-unused (defer)

In MudOS `func_spec` / man pages, **not** called from `std` or `adm/obj`. Do not implement unless a later scan finds a call.

Useful later: `unique_mapping`, `this_interactive` (now present), …

---

## MudOS packages

| Package | Notes |
| --- | --- |
| `sockets` | Stubs already registered |
| `parser` | `parse_command` implemented (subset used by `std/pet.c`) |
| Others | No calls in this scope |

---

## Remaining implement order

1. **`ed`** — full interactive editor if wizard edit matters
2. **`virtualp`** — return 1 once virtual objects exist
3. **`localtime`** — local TZ / `LT_GMTOFF` if HTTP/SMTP date stamps must be local
4. Socket family — real TCP instead of `EESOCKET` stubs (`netstat` already reports the stub)

After driver changes: `cargo build --release`, fully restart `rmudos`.

Tests: `compile_cmd_dirs`, `mortal_cmds_apply_without_unknown_efun`, `wizard_status_efuns::*`, `parse_command_pet_go_pattern`, `unique_array_groups_by_callback`, `localtime_epoch_is_utc_thursday`, `get_dir_and_file_size_match_mudos` in [`src/lib.rs`](../src/lib.rs).
