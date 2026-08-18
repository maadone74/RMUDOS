#include "../newbieville.h"
#include <std.h>

inherit ROOM;
void create() {
    ::create();
    
    set_property("light", 3);
    set_property("indoors", 0);
    set("short", "North Gates.");
    set("long", "These are the northern gates leading out from Newbieville. To the south is Majesty Avenue, which will take you to Town Square if you follow it.");
    add_exit(ROOMS+"majestyave", "south");
    /* Virtual overland rooms hang this driver on first load. */
}
