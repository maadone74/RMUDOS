# Creating and Using LPC Mudlibs

A **mudlib** is the game: rooms, players, NPCs, commands, and world logic written in LPC. The rmudos **driver** only provides the compiler, VM, networking, and efuns. Everything players experience lives under the mudlib root.

This guide walks through the bundled mudlib, then shows how to create your own.

---

## Mental model

```
mudlib/                 ← filesystem root (config `mudlib`)
  secure/master.c       ← object path /secure/master
  std/room.c            ← /std/room  (blueprint)
  std/user.c            ← /std/user  (cloned per player)
  room/void.c           ← /room/void (loaded once)
```

Rules of thumb:

1. **One `.c` file = one program**. Object path `/a/b` ↔ file `mudlib/a/b.c`.
2. **Master** owns world boot and player creation.
3. **Rooms** are usually `load_object` singletons with exits pointing at other object paths.
4. **Players** are `clone_object` instances of a user blueprint.
5. Prefer `inherit "/std/..."` for shared behavior; override `create()` in leaves.

---

## Minimal mudlib

A mud that boots and accepts one player needs three objects.

### 1. `secure/master.c`

```c
void create() {
    debug_message("master: create()");
}

void preload() {
    load_object("/room/start");
}

object connect() {
    return clone_object("/std/user");
}
```

### 2. `std/user.c`

```c
string name;

void create() {
    name = "Guest";
}

string query_name() {
    return name;
}

void logon() {
    write("Welcome.");
    move_object(load_object("/room/start"));
    write("> ");
}

int process_input(string line) {
    if (line == "quit") {
        write("Goodbye!");
        return 0;
    }
    if (line == "look") {
        write(environment()->short());
        write(environment()->long());
    } else {
        write("Try: look, quit");
    }
    write("> ");
    return 1;
}
```

### 3. `std/room.c` + `room/start.c`

```c
/* std/room.c */
string short_desc;
string long_desc;
mapping exits;

void create() {
    short_desc = "Somewhere";
    long_desc = "An empty place.";
    exits = ([]);
}

void set_short(string s) { short_desc = s; }
void set_long(string s) { long_desc = s; }
void add_exit(string dir, string dest) { exits[dir] = dest; }

string short() { return short_desc; }
string long() { return long_desc; }
string query_exit(string dir) { return exits[dir]; }
mapping query_exits() { return exits; }
```

```c
/* room/start.c */
inherit "/std/room";

void create() {
    set_short("Start");
    set_long("You stand in a quiet stone chamber.");
}
```

Point `config.toml` at this tree and run the driver. Connect with telnet.

---

## Bundled sample walkthrough

The shipped mudlib under `mudlib/` is a small three-room world.

| Object | Role |
| --- | --- |
| `/secure/master` | Boot + `connect()` clones `/std/user` |
| `/std/room` | Shared room API (`set_short`, `add_exit`, …) |
| `/std/user` | Interactive player: commands, movement, chat |
| `/room/void` | Starting room |
| `/room/tavern` | Linked east of void |
| `/room/street` | Linked south of void |

Boot:

1. Master `create()` logs via `debug_message`.
2. Master `preload()` loads void, tavern, and street so exits resolve immediately.
3. On connect, `clone_object("/std/user")` → interactive attached → `logon()` moves the player into `/room/void` and prints the room.

Study these files in order when learning:

1. `mudlib/secure/master.c`
2. `mudlib/std/room.c`
3. `mudlib/room/void.c` (inheritance pattern)
4. `mudlib/std/user.c` (command loop)

---

## Inheritance

```c
inherit "/std/room";

void create() {
    set_short("The Void");
    set_long("...");
    add_exit("east", "/room/tavern");
}
```

Behavior:

- Absolute inherits (`"/std/room"`) resolve from the mudlib root.
- Relative inherits resolve from the current object’s directory.
- Child functions **override** parent functions of the same name.
- Parent globals are merged into the child program.
- Child `create()` replaces parent `create()`; call parent helpers (`set_short`, …) explicitly — there is no automatic `::create()` chaining API beyond what you write.
- Cyclic inheritance is a compile error.

Recommended layout:

```
mudlib/
  std/          ← reusable blueprints (room, user, item, npc)
  secure/       ← master and privileged objects
  room/         ← concrete rooms inheriting std/room
  obj/          ← items / NPCs (optional)
  daemons/      ← singleton services (optional)
```

---

## Required applies for a playable mud

### Master (`/secure/master`)

| Function | Required? | Purpose |
| --- | --- | --- |
| `create()` | recommended | One-time master setup |
| `preload()` | recommended | `load_object` world rooms/daemons |
| `connect()` | **yes** for players | Return a fresh player object |

```c
object connect() {
    object user;
    user = clone_object("/std/user");
    return user;
}
```

### Player (`/std/user` or your clone blueprint)

| Function | Required? | Purpose |
| --- | --- | --- |
| `create()` | recommended | Default name / state |
| `logon()` | **yes** for welcome | Attach to starting room, print banner |
| `process_input(string)` | **yes** | Parse commands; return `0` to disconnect |

`process_input` contract:

- Argument: one line of player text (no trailing newline).
- Return `0` / falsy int → driver closes the connection and destructs the object.
- Return anything else → keep session.

### Rooms

Convention from the sample:

| Function | Purpose |
| --- | --- |
| `short()` / `long()` | Descriptions |
| `query_exit(dir)` / `query_exits()` | Movement graph |
| `init()` | Called after something moves in/out (via `move_object`) |
| `reset()` | Mudlib convention; **not** auto-invoked by this driver yet |

---

## Creating a new room

1. Inherit `/std/room` (or your own base).
2. In `create()`, set descriptions and exits to **object paths** (strings), not live objects.
3. Ensure master `preload()` (or first visitor) `load_object`s the room.

```c
inherit "/std/room";

void create() {
    set_short("Harbor Pier");
    set_long("Salt wind snaps the ropes. The street lies north.");
    add_exit("north", "/room/street");
}
```

Add a matching exit on the other side, and load the new room in `preload()`:

```c
void preload() {
    load_object("/room/void");
    load_object("/room/tavern");
    load_object("/room/street");
    load_object("/room/pier");   /* new */
}
```

Movement pattern (from `/std/user`):

```c
dest = env->query_exit(dir);
room = load_object(dest);
move_object(room);
```

`load_object` is safe if the room was already preloaded — it returns the existing blueprint.

---

## Creating a custom player / command set

Clone blueprint responsibilities:

1. Hold player state (name, stats, …) in globals.
2. Implement `logon()` for first-look UX.
3. Implement `process_input` as a command router.
4. Use efuns for I/O and world queries: `write`, `tell_room`, `users`, `environment`, `call_other` (`obj->fn(...)`).

Example: add an `emote` command.

```c
void emote_cmd(string msg) {
    object env;
    string myname;
    if (!msg || msg == "") {
        write("Emote what?");
        return;
    }
    myname = query_name();
    env = environment();
    write("You " + msg);
    if (env) {
        tell_room(env, capitalize(myname) + " " + msg, this_object());
    }
}

/* inside process_input: */
if (cmd == "emote" || cmd == "me") {
    emote_cmd(arg);
    write("> ");
    return 1;
}
```

Keep parsing helpers (`extract_cmd`, `extract_arg`) local or move them to an inherited `/std/living` when the mud grows.

---

## Creating NPCs and items

Pattern:

```c
/* std/npc.c */
string name;

void create() {
    name = "someone";
}

string query_name() {
    return name;
}

void heart_beat() {
    /* optional periodic AI — called every 2s if defined */
}
```

```c
/* obj/bartender.c */
inherit "/std/npc";

void create() {
    name = "bartender";
}
```

Spawn from a room `create()` or a daemon:

```c
object npc;
npc = clone_object("/obj/bartender");
move_object(npc, this_object());
```

Items follow the same clone + `move_object` pattern. Put `query_name()` (or `short()`) on anything you want listed in `look`.

---

## Daemons and singletons

Use `load_object` for unique services:

```c
/* daemons/channel.c */
void broadcast(string msg) {
    mixed u;
    int i;
    u = users();
    i = 0;
    while (i < sizeof(u)) {
        tell_object(u[i], "[chat] " + msg);
        i = i + 1;
    }
}
```

From a player command:

```c
load_object("/daemons/channel")->broadcast(arg);
```

Preload important daemons in master `preload()` so first use is cheap and fail-fast at boot.

---

## Messaging patterns

| Goal | Approach |
| --- | --- |
| Tell current player | `write(...)` |
| Tell one object | `tell_object(ob, msg)` |
| Tell everyone in a room | `tell_room(room, msg)` or `tell_room(room, msg, exclude)` |
| Typed channel-style | `message(class, text, target, exclude)` |
| Server log | `debug_message(msg)` |

`write` targets `this_player` when set, otherwise the current object. Interactive objects receive text on the telnet socket.

> **Note:** The sample defines `catch_tell(string)` on `/std/user`, but the driver currently delivers text by writing directly to the interactive channel. Prefer `write` / `tell_*` for reliability with this driver version.

---

## Heartbeats

Define `void heart_beat()` on any object. The backend ticks all such objects every **2 seconds**.

```c
int ticks;

void create() {
    ticks = 0;
}

void heart_beat() {
    ticks = ticks + 1;
    if (ticks % 15 == 0) {
        tell_room(this_object(), "A clock chimes somewhere nearby.");
    }
}
```

Avoid heavy work: each apply shares the `max_cost` instruction budget.

---

## Using a custom mudlib with rmudos

### Option A — replace / edit `mudlib/`

Keep `config.toml`:

```toml
mudlib = "mudlib"
master = "/secure/master"
```

Edit files in place; restart the driver after changes. **Objects are compiled when first loaded**; a running process does not hot-reload source. Restart after LPC edits.

### Option B — separate world tree

```
projects/
  rmudos/RMUDOS/          ← driver
  my_mudlib/              ← your world
    secure/master.c
    ...
```

```toml
mud_name = "MyMud"
mudlib = "../../my_mudlib"
master = "/secure/master"
port = 4001
```

Or CLI:

```bash
rmudos --config config.toml --mudlib /absolute/path/to/my_mudlib
```

### Option C — multiple muds, one binary

Ship one `rmudos` binary and many config files:

```bash
rmudos --config configs/rustmud.toml
rmudos --config configs/arena.toml --port 4002
```

Each process needs its own port and mudlib root.

---

## Checklist: new mudlib from scratch

1. Create mudlib root directory.
2. Add `secure/master.c` with `create`, `preload`, `connect`.
3. Add `std/user.c` with `logon` + `process_input`.
4. Add `std/room.c` and at least one start room.
5. Point `config.toml` `mudlib` / `master` at them.
6. `cargo run --release -- --config config.toml`
7. `telnet 127.0.0.1 <port>` and verify welcome + look + quit.
8. Expand rooms, commands, NPCs; keep preloading critical singletons.

Validate compilation without networking by temporarily adding paths to the `compile_mudlib` test in `src/lib.rs`, or by booting and watching startup errors.

---

## Design recommendations (senior notes)

- **Thin driver, fat mudlib** — keep gameplay in LPC; only extend Rust efuns when the mudlib cannot express something safely or efficiently.
- **Stable object paths** — exits and `load_object` strings are your public API; rename carefully.
- **Clone players, load rooms** — matches MudOS practice and the sample.
- **Centralize command parsing** early — `/std/user` grows quickly otherwise.
- **Fail at preload** — loading rooms at boot surfaces path typos before players connect.
- **Know the subset** — this LPC dialect is intentionally smaller than FluffOS; see [LPC.md](LPC.md) and [EFUNS.md](EFUNS.md).
- **No simul_efun / privilege layer yet** — do not assume MudOS `valid_read` / `valid_write` / simul_efun overlays exist. Treat the mudlib as trusted code.
- **Restart to reload** — there is no in-process `update` / recompile of blueprints in the current driver.

---

## Next

- [LPC.md](LPC.md) — syntax and types this compiler accepts
- [EFUNS.md](EFUNS.md) — every built-in callable from LPC
- [USAGE.md](USAGE.md) — run and configure the driver
