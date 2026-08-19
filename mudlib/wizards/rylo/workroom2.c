inherit "/std/locker_room.c";

void create()
{
    ::create();
    set("long", "This inherits from /std/locker_room.c");
    set_exits( ({ "/wizards/rylo/workroom.c"}), ({"north"}));
}

void reset()
{
    ::reset();
}