---
name: extend-lpc-compiler
description: Extend the rmudos LPC lexer, parser, AST, or bytecode codegen toward MudOS grammar. Use when adding syntax, inherit behavior, preprocessor, catch, foreach, or opcodes.
---

# Extend the LPC compiler

Files: `src/compiler/{lexer,parser,ast,codegen,mod}.rs`. Bytecode types live in `src/vm/program.rs`. Execution is `src/vm/interpret.rs`.

MudOS references: `lex.c` (tokens + cpp), `grammar.y.pre`, `compiler.c`, `generate.c`, `icode.c`, `trees.c`.

## Checklist

1. Write a failing test: `compiler::compile_source` (no inherit) or `compile_file_in` with a temp/mudlib fixture.
2. Lexer: token + comments/strings/escapes. Do not pretend to have a preprocessor unless you add one.
3. AST: new `Expr`/`Stmt` variant; keep nodes boring and Clone.
4. Parser: precedence must stay consistent with existing binary ops.
5. Codegen: emit `Op`s. Add a new `Op` only if existing ones cannot express the feature.
6. Interpreter: implement the `Op`. Cost it.
7. Inherit: merged globals unique; functions override by name; `local_functions` are this file only; cycles fail; `..` cannot leave mudlib.
8. Calls: existing local/inherit name → `Call`; else `CallEfun`. `obj->m(args)` → `call_other`.
9. Update `docs/LPC.md` with what authors can type.

## MudOS syntax still missing (typical)

`#include`/`#define`, `for`, `foreach`, `switch`/`case`, `do/while`, `catch`, `({` function pointers, classes, `nosave`, `varargs` enforcement, `private` inherit semantics.

Port one production at a time. Do not import yacc.

## Array literals

`({ ... })` may be rewritten to `[ ... ]` before parse. If you change array syntax, update lexer and parser together and keep mudlib compiling.
