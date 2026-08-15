// /secure/master.c — MudOS master object

void create() {
    debug_message("master: create()");
}

void preload() {
    debug_message("master: preloading world");
    load_object("/room/void");
    load_object("/room/tavern");
    load_object("/room/street");
}

object connect() {
    object user;
    user = clone_object("/std/user");
    return user;
}
