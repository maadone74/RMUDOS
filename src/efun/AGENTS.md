# Efun agents

Register every new built-in in `EfunTable::new`. Match MudOS names from `func_spec.c`.

Helpers already in this file: `require`, object/string conversion, `deliver_room`. Reuse them.

`this_object` is not registered here. Packages (db, sockets, uids) get their own modules when implemented.

Update `docs/EFUNS.md` in the same change.
