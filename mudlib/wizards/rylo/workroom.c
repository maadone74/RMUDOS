inherit "/std/vault_locker_room.c";

void create()
{
    ::create();
    set("long", "This inherits from /std/vault_locker_room.c");
    set_exits( ({ "/wizards/rylo/workroom2.c"}), ({"south"}) );
}

void reset()
{
    ::reset();
}