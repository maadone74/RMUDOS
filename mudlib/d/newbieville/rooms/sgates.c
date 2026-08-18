#include "../newbieville.h"
#include <std.h>

inherit ROOM;
void create() {
    ::create();
    
    set_property("light", 3);
    set_property("indoors", 0);
    set("short", "South Gates.");
    set("long", "These are the southern gates leading out from Newbieville. To the north is Common Street, which will take you to Town Square if you follow it.");
    add_exit(ROOMS+"commonstreet", "north");
    /* Virtual overland rooms hang this driver on first load. */
}
