# LPC Language Subset (rmudos)

rmudos compiles a **MudOS-inspired LPC subset** to bytecode. Types on locals/globals are largely documentary at runtime (values are dynamic `LpcValue`s), but the parser requires type keywords in declarations.

---

## Files and programs

- Source files use the `.c` extension.
- Object path `/foo/bar` ↔ `mudlib/foo/bar.c`.
- Comments: `// line` and `/* block */`.
- Top-level: `inherit` strings, global variable declarations, function declarations.

```c
inherit "/std/room";

string short_desc;
mapping exits;

void create() {
    short_desc = "Somewhere";
    exits = ([]);
}
```

---

## Types

| Keyword | Runtime value |
| --- | --- |
| `void` | Function return only |
| `int` | 64-bit integer |
| `float` | `f64` |
| `string` | UTF-8 string |
| `object` | Live object reference |
| `mapping` | String-keyed map |
| `mixed` | Any value |
| `function` | Accepted as a type name; no first-class function values yet |

Falsy values: `0` / null, integer `0`, float `0.0`, empty string, empty array, empty mapping, destructed object.

---

## Variables

Globals at file scope; locals at the start of a function body (declare before use in the usual LPC style):

```c
void example(string line) {
    int i;
    string out;
    i = 0;
    out = "";
}
```

Modifiers parsed but not fully enforced as a security model: `public`, `private`, `protected`, `static`, `nomask`, `varargs`.

---

## Literals

| Syntax | Meaning |
| --- | --- |
| `123`, `-1` | Integer |
| `1.5` | Float |
| `"text"` | String (escapes supported in lexer) |
| `({ 1, 2, 3 })` | Array (also rewritten from `({` / `})`) |
| `([ ])` / `([])` | Empty mapping |
| `([ "a": 1, "b": 2 ])` | Mapping (keys are strings) |

---

## Operators and control flow

Supported in broad strokes:

- Arithmetic: `+ - * / %`
- Comparison: `== != < <= > >=`
- Logic: `&& || !` (and truthiness via `!expr`)
- Assignment: `=`
- Indexing: `arr[i]`, `map[key]`, `string[i]` (as used in the sample mudlib)
- Slices: expression form exists in the compiler
- Calls: `fn(args)`, `obj->method(args)` (desugars to `call_other`)
- `this_object()` — current object (compiler intrinsic)
- Control: `if` / `else`, `while`, `return`, block `{ ... }`
- Ternary: `cond ? a : b`

Not a full FluffOS/MudOS grammar: expect gaps around closures, structs, comprehensive `foreach`, `#include` / `#define` preprocessor, and many package efuns.

---

## Functions

```c
int process_input(string line) {
    return 1;
}

void set_name(string n) {
    name = n;
}
```

- Overriding an inherited function replaces it for that program.
- Calling an unknown name is treated as an **efun** call at compile time.
- Local/inherited function names take precedence over efuns when both exist.

---

## Inheritance

```c
inherit "/std/room";
inherit "nearby";   /* relative to current object directory */
```

- Multiple inherits are supported; later entries merge over earlier ones according to codegen order.
- Cycles are rejected.
- Paths cannot escape the mudlib root with `..`.

---

## Objects and applies

Objects are instances of compiled programs. The driver and mudlib call named functions (“applies”) such as `create`, `logon`, and `process_input`. See [USAGE.md](USAGE.md) and [MUDLIB.md](MUDLIB.md).

---

## Compilation pipeline

1. Read `mudlib/<path>.c`
2. Lex → parse AST (`inherit`, globals, functions)
3. Recursively compile inherits
4. Generate bytecode `Program`
5. Instantiate object with global slots
6. Apply `create()`

Errors report parse/codegen context with the object path. Fix the LPC file and restart the driver (no hot reload).

---

## Practical limits vs classic MudOS

| Feature | rmudos today |
| --- | --- |
| Core objects / inherits | Yes |
| Mapping / array / string ops | Partial (see efuns) |
| simul_efun | No |
| `#include` / macros | No |
| Privileges / `valid_*` | No |
| save_object / restore_object | No |
| Full FluffOS packages | No — curated efun table only |
| `catch` / error handling LPC | Limited / not MudOS-complete |

Write mudlibs to this subset; porting a large FluffOS lib will require trimming and efun shims.
