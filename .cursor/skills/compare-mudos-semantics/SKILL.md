---
name: compare-mudos-semantics
description: Compare rmudos behavior with MudOS v22.2b14 C sources. Use when debugging LPC incompatibilities, verifying an efun/apply, or deciding if a difference is intentional.
---

# Compare rmudos to MudOS

1. Clone or reuse https://github.com/lnsoso/mudos (`/tmp/mudos`).
2. Identify the feature (efun name, apply, operator, inherit rule).
3. Read C: `func_spec.c` → implementation `.c`; or `applies` → call sites (`backend.c`, `comm.c`, `simulate.c`, `interpret.c`).
4. Read Rust: mapped module from skill `port-mudos-layer` file map.
5. Produce a short table:

   | Case | MudOS | rmudos | Action |
   | --- | --- | --- | --- |
   | … | … | … | match / document / defer |

6. Call out `options.h` flags that change C behavior (`NO_ADD_ACTION`, `NO_ENVIRONMENT`, `COMPAT_32`, `SENSIBLE_MODIFIERS`, `PACKAGE_*`).
7. Intentional differences (Tokio, no malloc, string-key mappings, curated efuns) stay unless the user asked for full compat.

Do not paste large C files into the Rust tree. Quote small snippets in the PR/test comments only when they document a subtle rule (truthiness, default args).

If C is unavailable (network), use `docs/` and this skill’s sibling references; say what could not be verified.
