#include "../newbieville.h"
#include <std.h>

inherit ROOM;
void create() {
    ::create();

    /* Bulletin board deferred: cloning /std/bboard + BBOARD_D on first
     * enter can hang on cloud-synced board save files. */
    set_property("light", 4);
    set_property("indoors", 1);
    set("short", "The Castle of Tailwind");
    set("long", "This is the Castle of Lord Tailwind. You see a staircase leading upwards, and nothing else.");
    add_exit(ROOMS+"upperfloor", "up");
    add_exit(ROOMS+"townsquare", "out");
}
