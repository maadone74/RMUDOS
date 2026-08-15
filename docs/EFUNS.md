# Efun Reference (rmudos)

Efuns are built-in functions implemented in Rust (`src/efun/mod.rs`) and callable from LPC. If a name is not an inherited/local function, the compiler emits an efun call.

Argument positions are 1-based in error messages. Unless noted, insufficient arguments error.

---

## Output and messaging

### `write(...)`

Concatenate all arguments to a string and send to `this_player` if set, otherwise to the current object’s interactive channel.

Returns `1`.

### `say(message)`

Send `message` to all objects in the actor’s environment except the actor (`this_player` or current object).

Returns `1`.

### `tell_object(target, message)`

Write `message` to a live object.

Returns `1`.

### `tell_room(room, message)` / `tell_room(room, message, exclude)`

Deliver `message` to inventory of `room`. `room` may be an object or path (loaded if needed). Optional `exclude` is an object or array of objects not to notify.

Returns `1`.

### `message(class, text, target, exclude?)`

Deliver `text` to `target` (object, array, or room path). `class` is accepted for MudOS familiarity but not specially interpreted. Optional exclude list supported.

Returns `1`.

### `debug_message(msg)`

Log at info level on tracing target `mudlib`.

Returns `1`.

---

## Strings and collections

| Efun | Signature (conceptual) | Result |
| --- | --- | --- |
| `capitalize(s)` | string → string | Uppercase first character |
| `lower_case(s)` | string → string | Lowercase |
| `strlen(s)` | string → int | Character count |
| `explode(s, sep)` | string, string → array | Split; empty sep → characters; empty parts dropped |
| `implode(arr, sep)` | array, string → string | Join |
| `member_array(item, arr\|str, start?)` | → int | Index or `-1` |
| `sizeof(x)` | string\|array\|mapping\|null → int | Length (`null` → 0) |
| `keys(m)` | mapping → array | Key strings |
| `values(m)` | mapping → array | Values |
| `sprintf(fmt, ...)` | string, … → string | See below |
| `atoi(s)` | string → int | Parse int or `0` |
| `to_string(x)` | any → string | Display conversion |
| `typeof(x)` | any → string | `null`/`int`/`float`/`string`/`array`/`mapping`/`object` |

### `sprintf`

Supported directives: `%%`, `%s`, `%d`/`%i`, `%f`, `%O` (LPC repr). Optional width and `-` left align (e.g. `%-10s`).

---

## Objects and world

| Efun | Behavior |
| --- | --- |
| `clone_object(path)` | New instance; runs `create()`; returns object |
| `load_object(path)` | Blueprint singleton; compiles/creates if needed |
| `find_object(path)` | Blueprint lookup or `0` |
| `destruct(obj)` | Destroy object; detach inventory/env |
| `move_object(dest)` | Move current object into `dest` |
| `move_object(obj, dest)` | Move `obj` into `dest` |
| `environment()` / `environment(obj)` | Container or `0` |
| `all_inventory()` / `all_inventory(obj)` | Array of contents |
| `file_name()` / `file_name(obj)` | Path, or `path#clone` for clones |
| `this_player()` | Interactive actor for this apply, or `0` |
| `previous_object()` | Previous object context, or `0` |
| `users()` | Array of interactive objects |
| `call_other(obj\|path, "fn", ...)` | Call function on target (also `obj->fn(...)`) |
| `time()` | Unix seconds (`int`) |
| `shutdown()` | Request driver shutdown |

`this_object()` is a **compiler intrinsic**, not an efun table entry.

Path arguments are normalized (`\`, optional `.c`, leading `/`).

---

## Cost and errors

Each apply runs with `max_cost` from config. Exceeding the budget aborts the apply. Efun failures surface as apply errors; the telnet backend prints `Error: …` to the player for `process_input` failures and continues the session unless the object was destructed.

---

## Extending efuns

To add a built-in:

1. Implement `fn(interpreter, args) -> Result<LpcValue>` in `src/efun/mod.rs`.
2. Register it in `EfunTable::new`.
3. Document it here.
4. Prefer mudlib-level helpers when possible.

There is currently **no** simul_efun layer and **no** package loading system.
