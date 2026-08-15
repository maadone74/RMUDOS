inherit "/std/room";

void create() {
    set_short("Market Street");
    set_long("Worn cobbles run between shuttered stalls. North leads into the void; west reaches the tavern.");
    add_exit("north", "/room/void");
    add_exit("west", "/room/tavern");
}
