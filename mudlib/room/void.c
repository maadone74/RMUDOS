inherit "/std/room";

void create() {
    set_short("The Void");
    set_long("You float in a quiet grey void. A warm light to the east hints at a tavern, and a cobbled path leads south toward a street.");
    add_exit("east", "/room/tavern");
    add_exit("south", "/room/street");
}
