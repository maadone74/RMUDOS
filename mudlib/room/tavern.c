inherit "/std/room";

void create() {
    set_short("The Rusty Anchor Tavern");
    set_long("A low-beamed tavern smells of ale and lamp oil. Adventurers mutter over maps. The void waits to the west.");
    add_exit("west", "/room/void");
    add_exit("south", "/room/street");
}
