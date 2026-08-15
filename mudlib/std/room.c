// /std/room.c — basic room blueprint

string short_desc;
string long_desc;
mapping exits;

void create() {
    short_desc = "Somewhere";
    long_desc = "An empty place.";
    exits = ([]);
}

void set_short(string s) {
    short_desc = s;
}

void set_long(string s) {
    long_desc = s;
}

void add_exit(string dir, string dest) {
    if (!exits) {
        exits = ([]);
    }
    exits[dir] = dest;
}

string short() {
    if (!short_desc) {
        return "Somewhere";
    }
    return short_desc;
}

string long() {
    if (!long_desc) {
        return "An empty place.";
    }
    return long_desc;
}

mapping query_exits() {
    if (!exits) {
        exits = ([]);
    }
    return exits;
}

string query_exit(string dir) {
    if (!exits) {
        exits = ([]);
    }
    return exits[dir];
}

void init() {
}

void reset() {
}
