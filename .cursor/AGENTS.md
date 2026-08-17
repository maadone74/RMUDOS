# RMUDOS agent notes

Rust MudOS-compatible LPC driver + Nightmare/Darke mudlib. Crate lives in `RMUDOS/`.

## Rules

Project rules are in `.cursor/rules/`. LPC language detail: `RMUDOS/.cursor/rules/lpc-spec.mdc` (mudlib only).

## Skills

Use when relevant:

- `.cursor/skills/rmudos-mudos-compat` — efun/VM/`add_action`/`this_player`
- `.cursor/skills/rmudos-playability` — hangs, movement, newchar, OneDrive
- `.cursor/skills/rmudos-mudlib-patch` — Nightmare LPC edits

## Working copy

Prefer `cargo build --release` in `RMUDOS/` and a full process restart after driver changes. Do not add `get_dir` on command paths; this tree is often on OneDrive.
