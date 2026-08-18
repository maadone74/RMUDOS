#!/usr/bin/env python3
"""Scan mudlib/std + adm/obj for MudOS efun names vs RMUDOS registrations."""
from __future__ import annotations

import re
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MUDLIB = ROOT / "mudlib"

# MudOS v22 func_spec.c public efun names (not _internal aliases).
# Aliases listed separately. Packages (mysql, sockets extra, math already in core) noted in docs.
MUDOS_CORE = {
    "bind", "this_player", "this_interactive", "this_user", "previous_object",
    "all_previous_objects", "call_stack", "sizeof", "strlen", "destruct",
    "file_name", "capitalize", "explode", "implode", "call_out", "member_array",
    "input_to", "random", "environment", "all_inventory", "deep_inventory",
    "first_inventory", "next_inventory", "say", "tell_room", "present",
    "move_object", "add_action", "query_verb", "command", "remove_action",
    "living", "commands", "disable_commands", "enable_commands",
    "set_living_name", "livings", "find_living", "find_player", "notify_fail",
    "lower_case", "replace_string", "restore_object", "save_object",
    "save_variable", "restore_variable", "users", "get_dir", "strsrch",
    "write", "tell_object", "shout", "receive", "message", "find_object",
    "load_object", "find_call_out", "allocate_mapping", "values", "keys",
    "map_delete", "match_path", "clonep", "intp", "undefinedp", "nullp",
    "floatp", "stringp", "virtualp", "functionp", "pointerp", "arrayp",
    "objectp", "classp", "typeof", "bufferp", "allocate_buffer", "inherits",
    "replace_program", "regexp", "reg_assoc", "allocate", "call_out_info",
    "crc32", "read_buffer", "write_buffer", "write_file", "rename",
    "write_bytes", "file_size", "read_bytes", "read_file", "cp", "link",
    "mkdir", "rm", "rmdir", "clear_bit", "test_bit", "set_bit", "next_bit",
    "crypt", "oldcrypt", "ctime", "exec", "localtime", "function_exists",
    "objects", "query_host_name", "query_idle", "query_ip_name",
    "query_ip_number", "snoop", "query_snoop", "query_snooping",
    "remove_call_out", "set_heart_beat", "query_heart_beat", "set_hide",
    "generate_source", "set_reset", "shadow", "query_shadowing", "sort_array",
    "throw", "time", "unique_array", "unique_mapping", "deep_inherit_list",
    "shallow_inherit_list", "inherit_list", "printf", "sprintf", "mapp",
    "stat", "interactive", "in_edit", "in_input", "userp", "enable_wizard",
    "disable_wizard", "wizardp", "master", "memory_info", "get_config",
    "query_privs", "set_privs", "get_char", "children", "reload_object",
    "error", "uptime", "strcmp", "rusage", "flush_messages", "ed",
    "cache_stats", "filter", "filter_array", "filter_mapping", "map",
    "map_mapping", "map_array", "malloc_status", "mud_status", "dumpallobj",
    "dump_file_descriptors", "query_load_average", "set_light", "origin",
    "reclaim_objects", "set_eval_limit", "reset_eval_cost", "eval_cost",
    "max_eval_cost", "set_debug_level", "opcprof", "function_profile",
    "swap", "resolve", "shutdown", "evaluate", "to_int", "to_float",
    "clone_object", "new", "this_object", "call_other",
}

# Names in mudlib/doc/efun that are not in core func_spec (Nightmare extras / packages).
DOC_EXTRA = {
    "author_stats", "domain_stats", "set_author", "set_living_name",
    "export_uid", "geteuid", "getuid", "seteuid", "dump_socket_status",
    "socket_accept", "socket_acquire", "socket_address", "socket_bind",
    "socket_close", "socket_connect", "socket_create", "socket_error",
    "socket_listen", "socket_release", "socket_write", "process_string",
    "process_value", "parse_command", "break_string", "tail", "debug_info",
    "debugmalloc", "dump_prog", "moncontrol", "refs", "set_malloc_mask",
    "time_expression", "trace", "traceprefix", "errorp", "file_exists",
    "base_name", "copy", "acos", "asin", "atan", "ceil", "cos", "exp",
    "floor", "log", "pow", "sin", "sqrt", "tan", "each", "debug_message",
    "clear_actions", "next_shadow", "read_database", "sscanf", "user_exists",
    "version", "mud_name", "mudlib", "mudlib_version", "bind",
}

RUST = {
    "write", "say", "tell_object", "tell_room", "message", "capitalize",
    "lower_case", "strlen", "explode", "implode", "member_array", "sizeof",
    "filter_array", "map_array", "evaluate", "keys", "values", "clone_object",
    "load_object", "find_object", "destruct", "move_object", "environment",
    "all_inventory", "file_name", "this_player", "previous_object", "origin",
    "users", "call_other", "getuid", "geteuid", "seteuid", "enable_commands",
    "disable_commands", "living", "interactive", "wizardp", "userp", "sprintf",
    "printf", "atoi", "to_string", "typeof", "functionp", "stringp", "objectp",
    "intp", "pointerp", "mapp", "time", "random", "debug_message", "shutdown",
    "set_heart_beat", "query_heart_beat", "input_to", "new", "deep_inventory",
    "present", "query_idle", "reset_eval_cost", "throw", "nullp", "undefinedp",
    "master", "receive", "error", "function_exists", "export_uid",
    "call_out", "remove_call_out", "find_call_out", "read_file", "write_file",
    "file_size", "file_exists", "get_dir", "mkdir", "rm", "cp", "rename",
    "read_database", "sscanf", "sscanf_values", "replace_string", "strsrch",
    "to_int", "to_float", "pow", "sqrt", "sin", "cos", "tan", "asin", "acos",
    "atan", "log", "exp", "floor", "ceil", "ctime", "allocate",
    "allocate_mapping", "map_delete", "copy", "sort_array", "base_name",
    "regexp", "add_action", "clear_actions", "notify_fail", "query_verb",
    "command", "commands", "exec", "crypt", "user_exists", "find_player",
    "find_living", "set_living_name", "enable_wizard", "version", "mud_name",
    "mudlib", "mudlib_version", "query_ip_number", "query_ip_name",
    "save_object", "restore_object", "save_variable", "restore_variable",
    "shadow", "query_shadowing", "next_shadow", "bind", "map", "filter",
    "socket_create", "socket_bind", "socket_listen", "socket_accept",
    "socket_connect", "socket_write", "socket_close", "socket_release",
    "socket_acquire", "socket_address", "socket_error", "dump_socket_status",
}

STUBS = {
    "socket_bind", "socket_listen", "socket_accept", "socket_connect",
    "socket_write", "socket_release", "socket_acquire",
}

INTRINSICS = {"this_object", "catch"}

# Simuls actually #included from adm/obj/simul_efun.c
SIMUL = {
    "article", "base_name", "log_file", "wrap", "hiddenp", "identify",
    "creator_file", "exclude_array", "find_object_or_load", "format_string",
    "query_idle_string", "path_file", "substr", "distinct_array", "user_path",
    "resolv_path", "member_group", "archp", "owner_euid",
    "member_array", "destruct", "snoop", "exec", "query_snoop", "query_snooping",
    "write", "deep_inherit_list", "message",
    "copy", "parse_objects", "arrange_string", "total_light", "to_object",
    "tell_player", "percent", "ansi_str", "ansi_inventory", "ansi_item",
    "inverse", "bold", "underscore", "cls", "clear_line", "cursor_up",
    "red", "blue", "green", "yellow", "cyan", "magenta", "erase_line",
    "effective_light", "high_mortalp", "interact",
    "query_night", "day", "date", "month", "year", "minutes", "season", "hour",
    "atoi", "file_exists", "absolute_value", "mud_name", "version",
    "architecture", "mudlib", "mudlib_version", "query_host_port",
    "get_object", "get_objects", "alignment_ok", "visible", "domain_master",
    "pluralize", "format_page", "ordinal", "personal_log", "consolidate",
    "user_exists", "read_database", "ambassadorp",
    "say", "tell_object", "tell_room", "shout",
    "cat", "extract", "first_inventory", "next_inventory",
    "translate", "currency_rate", "currency_inflation", "currency_weight",
    "mud_currencies", "event", "event_pending", "leaderp",
    "possessive", "nominative", "objective", "skill_contest", "do_critical",
    "eliminate_colour", "delayed_call", "delayed_call_info",
    "remove_delayed_call", "delayed_call_obj", "capp", "allcapp", "syntax",
    "verify_dir_exists", "write_border", "legendp", "herop",
}

# Simul that still requires a missing driver efun via efun::
SIMUL_NEEDS_EFUN = {
    "deep_inherit_list": "deep_inherit_list",
    "snoop": "snoop",
    "query_snoop": "query_snoop",
    "query_snooping": "query_snooping",
}

P0_FILES = {
    "adm/obj/login.c", "adm/obj/master.c", "adm/obj/simul_efun.c",
    "std/user.c", "std/living.c", "std/move.c", "std/container.c",
    "std/room.c", "std/room/exits.c",
}
P1_PREFIXES = (
    "std/living/", "std/monster.c", "std/npc_shop.c", "std/vault.c",
    "std/locker.c", "std/user/", "std/room/", "std/combat",
)

CALL_RE = re.compile(
    r"(?:efun\s*::\s*)?([a-zA-Z_][a-zA-Z0-9_]*)\s*\("
)
ARROW_RE = re.compile(r"->\s*[a-zA-Z_][a-zA-Z0-9_]*\s*\(")
COMMENT_RE = re.compile(r"/\*.*?\*/", re.S)
LINE_COMMENT_RE = re.compile(r"//.*?$", re.M)
STRING_RE = re.compile(r'"(?:\\.|[^"\\])*"')


def strip_noise(text: str) -> str:
    text = COMMENT_RE.sub(" ", text)
    text = LINE_COMMENT_RE.sub(" ", text)
    text = STRING_RE.sub('""', text)
    return text


def rel(path: Path) -> str:
    return str(path.relative_to(MUDLIB)).replace("\\", "/")


def scan_file(path: Path, names: set[str]) -> dict[str, int]:
    text = strip_noise(path.read_text(errors="replace"))
    # Mask obj->fun( and inherit::fun( so we only count bare / efun:: calls.
    masked = ARROW_RE.sub("->X(", text)
    masked = re.sub(r"(?<!efun)::\s*[a-zA-Z_][a-zA-Z0-9_]*\s*\(", "::X(", masked)
    hits: dict[str, int] = defaultdict(int)
    for m in CALL_RE.finditer(masked):
        name = m.group(1)
        if name not in names:
            continue
        before = masked[max(0, m.start() - 80) : m.start()]
        if re.search(
            r"(?:void|int|string|object|mixed|float|mapping|function|buffer)\s*\*?\s*$",
            before,
        ):
            continue
        hits[name] += 1
    return hits


def tier(path: str) -> str:
    if path in P0_FILES:
        return "P0"
    if any(path == p or path.startswith(p) for p in P1_PREFIXES):
        return "P1"
    if path.startswith("adm/obj/"):
        return "P0"
    return "P2"


def main() -> None:
    catalog = MUDOS_CORE | DOC_EXTRA | RUST
    files = list((MUDLIB / "std").rglob("*.c")) + list((MUDLIB / "adm" / "obj").glob("*.c"))
    usage: dict[str, dict[str, int]] = defaultdict(lambda: defaultdict(int))
    for path in files:
        if not path.is_file():
            continue
        r = rel(path)
        for name, n in scan_file(path, catalog).items():
            usage[name][r] += n

    print("=== USED IN SCOPE ===")
    for name in sorted(usage):
        files_hit = usage[name]
        in_rust = name in RUST
        in_simul = name in SIMUL
        needs = SIMUL_NEEDS_EFUN.get(name)
        stub = name in STUBS
        intrinsic = name in INTRINSICS
        tiers = sorted({tier(f) for f in files_hit})
        sample = ", ".join(sorted(files_hit)[:4])
        extra = ""
        if needs and not in_rust:
            extra = f" simul-wraps-missing-efun::{needs}"
        elif in_simul and not in_rust:
            extra = " simul-complete"
        elif stub:
            extra = " STUB"
        elif in_rust:
            extra = " rust"
        elif intrinsic:
            extra = " intrinsic"
        else:
            extra = " MISSING"
        print(f"{name}\t{sum(files_hit.values())}\t{','.join(tiers)}\t{extra}\t{sample}")

    used = set(usage)
    missing_used = []
    for name in sorted(used):
        if name in INTRINSICS:
            continue
        if name in RUST and name not in STUBS:
            continue
        if name in SIMUL and name not in SIMUL_NEEDS_EFUN and name not in STUBS:
            continue
        if name in STUBS:
            continue
        missing_used.append(name)

    print("\n=== MISSING-USED (driver efun needed) ===")
    for name in missing_used:
        print(name, dict(usage[name]))

    print("\n=== STUBS USED ===")
    for name in sorted(STUBS & used):
        print(name, dict(usage[name]))

    man = {p.name for p in (MUDLIB / "doc" / "efun").rglob("*") if p.is_file()}
    unused = sorted((MUDOS_CORE | man) - used - RUST - SIMUL - INTRINSICS)
    print("\n=== MISSING-UNUSED count ===", len(unused))


if __name__ == "__main__":
    main()
