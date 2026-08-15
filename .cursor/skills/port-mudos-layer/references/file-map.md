# MudOS C file → rmudos Rust module

Upstream: https://github.com/lnsoso/mudos

| MudOS | Role | rmudos |
| --- | --- | --- |
| `main.c`, `rc.c`, `config.h`, `options.h` | startup, runtime config | `src/main.rs`, `src/config.rs`, `config.toml` |
| `backend.c` | game loop, heartbeat, idle | `src/backend.rs` |
| `comm.c`, `telnet.h` | player sockets, IAC | `src/net/telnet.rs` |
| `simulate.c` | load/clone/destruct, errors | `src/simulate.rs`, `MudWorld` in `src/vm/mod.rs` |
| `object.c`, `object.h`, `otable.c` | `object_t`, hash names | `src/vm/object.rs`, `MudWorld` maps |
| `lpc.h` | `svalue_t` | `src/vm/value.rs` |
| `program.c`, `program.h` | compiled program | `src/vm/program.rs` |
| `interpret.c`, `interpret.h` | stack VM | `src/vm/interpret.rs` |
| `applies` | driver callbacks | `src/vm/apply.rs` plus string names |
| `lex.c`, `lex.h` | LPC lexer + preprocessor | `src/compiler/lexer.rs` (no cpp yet) |
| `grammar.y.pre`, `compiler.c`, `trees.c` | parser / AST | `src/compiler/parser.rs`, `ast.rs` |
| `generate.c`, `icode.c` | bytecode | `src/compiler/codegen.rs` |
| `func_spec.c`, `op_spec.c` | efun prototypes / operators | `EfunTable` + `Op` |
| `efuns_main.c`, `efuns_port.c`, `eoperators.c` | core efuns | `src/efun/mod.rs` |
| `sprintf.c` | `sprintf`/`printf` | `sprintf` in efun module |
| `array.c`, `mapping.c`, `stralloc.c`, `buffer.c` | collections | `LpcValue` variants |
| `call_out.c` | delayed applies | **missing** — add next to backend/world |
| `add_action.c` | verbs / living | **missing** (MudOS `NO_ADD_ACTION` optional) |
| `master.c` | master applies from C | `simulate.rs` + mudlib `/secure/master.c` |
| `simul_efun.c` | simul efun object | **missing** |
| `file.c` | mudlib file I/O efuns | **missing** except none |
| `socket_efuns.c`, `packages/sockets.c` | LPC sockets | **missing** |
| `packages/db.c` | MySQL (lnsoso) | **missing** — optional later |
| `packages/uids.c` | uid/euid | **missing** |
| `packages/parser.c` | parse_command | **missing** |
| `packages/math.c`, `contrib.c`, `develop.c` | extra efuns | **missing** |
| `ed.c` | in-driver editor | skip |
| `swap.c`, `binaries.c`, `ccode.c` | swap, save binary, LPC-to-C | skip |
| `bsdmalloc.c`, `smalloc.c`, `debugmalloc.c` | allocators | skip (use Rust heap) |
| `addr_server.c` | hostname lookup daemon | skip or optional later |
| `testsuite/` | LPC driver tests | `src/lib.rs` tests + `mudlib/` |

## Object flags worth knowing (`object.h`)

`O_HEART_BEAT`, `O_CLONE`, `O_DESTRUCTED`, `O_VIRTUAL`, `O_HIDDEN`, `O_ENABLE_COMMANDS` / `O_CATCH_TELL`. rmudos currently tracks destructed, clone number, interactive, and heartbeat-by-function-presence.

## Master / object applies

See skill `add-apply` references. Boot path in C: load master, `__INIT`/`create`, `flag`, `preload`/`epilog`. rmudos: `create` via `load_object`, then `preload`.
