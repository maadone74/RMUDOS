// /std/user.c — interactive player object

string name;

void create() {
    name = "Guest";
}

void set_name(string n) {
    name = n;
}

string query_name() {
    return name;
}

string extract_arg(string line) {
    int i;
    string out;
    i = 0;
    while (i < strlen(line) && line[i] != " ") {
        i = i + 1;
    }
    while (i < strlen(line) && line[i] == " ") {
        i = i + 1;
    }
    out = "";
    while (i < strlen(line)) {
        out = out + line[i];
        i = i + 1;
    }
    return out;
}

string extract_cmd(string line) {
    int i;
    string out;
    i = 0;
    out = "";
    while (i < strlen(line) && line[i] != " ") {
        out = out + line[i];
        i = i + 1;
    }
    return lower_case(out);
}

void look_cmd() {
    object env;
    mapping ex;
    mixed dirs;
    mixed inv;
    int i;
    string nm;

    env = environment(this_object());
    if (!env) {
        write("You are nowhere.");
        return;
    }
    write(env->short());
    write(env->long());
    ex = env->query_exits();
    dirs = keys(ex);
    if (sizeof(dirs) > 0) {
        write("Exits: " + implode(dirs, ", "));
    } else {
        write("Exits: none");
    }
    inv = all_inventory(env);
    i = 0;
    while (i < sizeof(inv)) {
        if (inv[i] != this_object()) {
            nm = inv[i]->query_name();
            if (nm && nm != "") {
                write(capitalize(nm) + " is here.");
            }
        }
        i = i + 1;
    }
}

void go_cmd(string dir) {
    object env;
    string dest;
    object room;
    string myname;

    if (!dir || dir == "") {
        write("Go where?");
        return;
    }
    env = environment(this_object());
    if (!env) {
        write("You can't move from nowhere.");
        return;
    }
    dest = env->query_exit(dir);
    if (!dest || dest == "") {
        write("You can't go that way.");
        return;
    }
    room = load_object(dest);
    myname = query_name();
    tell_room(env, capitalize(myname) + " leaves " + dir + ".", this_object());
    move_object(room);
    tell_room(room, capitalize(myname) + " arrives.", this_object());
    look_cmd();
}

void say_cmd(string msg) {
    object env;
    string myname;
    if (!msg || msg == "") {
        write("Say what?");
        return;
    }
    myname = query_name();
    env = environment(this_object());
    write("You say: " + msg);
    if (env) {
        tell_room(env, capitalize(myname) + " says: " + msg, this_object());
    }
}

void who_cmd() {
    mixed u;
    int i;
    u = users();
    write("Players:");
    i = 0;
    while (i < sizeof(u)) {
        write("  - " + capitalize(u[i]->query_name()));
        i = i + 1;
    }
}

void logon() {
    object start;
    write("");
    write("========================================");
    write("  Welcome to RustMud (rmudos driver)");
    write("  MudOS-inspired LPC on Rust");
    write("========================================");
    write("");
    write("Commands: look, go <dir>, say <text>, who, quit, help");
    write("");
    start = load_object("/room/void");
    move_object(start);
    look_cmd();
    write("> ");
}

int process_input(string line) {
    string cmd;
    string arg;

    if (!line || line == "") {
        write("> ");
        return 1;
    }

    cmd = extract_cmd(line);
    arg = extract_arg(line);

    if (cmd == "quit" || cmd == "logout") {
        write("Goodbye!");
        return 0;
    }
    if (cmd == "look" || cmd == "l") {
        look_cmd();
        write("> ");
        return 1;
    }
    if (cmd == "go" || cmd == "move") {
        go_cmd(arg);
        write("> ");
        return 1;
    }
    if (cmd == "north" || cmd == "south" || cmd == "east" || cmd == "west" || cmd == "up" || cmd == "down") {
        go_cmd(cmd);
        write("> ");
        return 1;
    }
    if (cmd == "say") {
        say_cmd(arg);
        write("> ");
        return 1;
    }
    if (cmd == "who") {
        who_cmd();
        write("> ");
        return 1;
    }
    if (cmd == "help") {
        write("look / l          - describe room");
        write("go <dir> / north  - move");
        write("say <text>        - speak");
        write("who               - list players");
        write("quit              - disconnect");
        write("> ");
        return 1;
    }

    write("Unknown command. Try 'help'.");
    write("> ");
    return 1;
}

void catch_tell(string msg) {
    write(msg);
}
