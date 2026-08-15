# Mudlib agents

Sample world for the rmudos LPC subset only (`docs/LPC.md`, `docs/EFUNS.md`, `docs/MUDLIB.md`).

Master `/secure/master.c`: `create`, `preload`, `connect`.
Users: `logon`, `process_input` (return 0 to disconnect).
Rooms: inherit `/std/room`.

If LPC fails to compile, the feature is probably missing in the driver — implement the driver first.
