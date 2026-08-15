# Compiler agents

When editing this directory, keep the pipeline `lex → parse → inherit resolve → codegen`.

- `compile_file_in` is the only inherit-capable entry.
- New syntax needs: token (lexer) → AST node → parser production → opcode or existing ops in codegen → `docs/LPC.md` + a compile test.
- `this_object()` → `Op::ThisObject`. Unknown names → `Op::CallEfun`.
- MudOS grammar reference: `grammar.y.pre`, `lex.c` in https://github.com/lnsoso/mudos

Do not evaluate LPC here; that is `src/vm/interpret.rs`.
