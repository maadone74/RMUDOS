//	/std/container.c

//	from the Nightmare mudlib

//	code inherited by all items which can hold things

//	created by Sulam@TMI for the TMI mudlib distribution

//	13 january 1992

// Bug with invisible objects fixed by Pallando 93-07-14



#include <std.h>



inherit OBJECT;



private nosave int internal_encumbrance;

int possible_to_close;

int is_closed;

int max_internal_encumbrance;



void set_max_internal_encumbrance(int max);

int query_max_internal_encumbrance();

int query_internal_encumbrance();

int receive_objects();

int add_encumbrance(int enc);



// Implement an object that can contain things.

// The 'remove()' function is implemented by the move.c object

// and will take care of any objects inside this object.



void set_max_internal_encumbrance(int max) {

    max_internal_encumbrance = max;

}



void set_possible_to_close(int pos) {

    possible_to_close = pos;

}



int query_internal_encumbrance() {

    return internal_encumbrance;

}



int query_max_internal_encumbrance() {

    return max_internal_encumbrance;

}



int toggle_closed() {

    if (possible_to_close) {

      if (is_closed) is_closed = 0;

      else is_closed = 1;

      return 1;

    }

    else return 0;

}



// This function is called from move()



int receive_objects() {

    if (is_closed) return 0;

    return 1;

}



int add_encumbrance(int enc) {

    if( !max_internal_encumbrance ) return 1;

    if( enc + internal_encumbrance > max_internal_encumbrance ) return 0;

    internal_encumbrance += enc;

    return 1;

}



string describe_living_contents(object *exclude) {

    object *inv, *livs;

    mapping list;

    string *shorts;

    string tmp, ret;

    int i, x;



  if(!exclude) exclude = ({});

    /* Do not use filter_array functionals or `arr -= ({ 0 })` here:
     * object PartialEq / array-subtract has hung this driver after the
     * room description is already sent. */
    inv = all_inventory(this_object());
    livs = ({});
    i = sizeof(inv);
    while(i--) {
	if(!inv[i] || !living(inv[i])) continue;
	if(exclude && sizeof(exclude)) {
	    x = sizeof(exclude);
	    while(x--)
		if(exclude[x] == inv[i]) break;
	    if(x >= 0) continue;
	}
	livs += ({ inv[i] });
    }

    i = sizeof(livs);

    if(!i) return "";

    list = ([]);

    while(i--) {

	x = (int)previous_object()->query_skill("perception") + 

	  ((effective_light(previous_object()) - 2) * 8);

   	if(livs[i]->query_hiding() && skill_contest((int)livs[i]->query_hiding(),

						   x, 1) != 2 &&

	   !wizardp(previous_object()))

	  continue;

        tmp = livs[i]->query_short();

        if(!stringp(tmp) || tmp == "") {

            if(wizardp(livs[i]) || random(101)> (int)previous_object()->query_level()) continue;

            tmp = "a shadow";

        }

	if(livs[i]->query_invis() && !previous_object()->query("see invis")) 

	  continue;

        if(!list[tmp]) list[tmp] = ({ livs[i] });

        else list[tmp] += ({ livs[i] });

    }

    i = sizeof(shorts = keys(list));

    ret = "";

    while(i--) {

        if((x=sizeof(list[shorts[i]])) < 2) ret += shorts[i]+"\n";

        else ret += capitalize(consolidate(x, shorts[i]))+"\n";

    }

    return ret;

}



string describe_item_contents(object *exclude) {

    object *inv, *items;

    mapping list;

    string ret, tmp;

    string *shorts;

    int i, x;



    inv = all_inventory(this_object());
    items = ({});
    i = sizeof(inv);
    while(i--) {
	if(!inv[i] || living(inv[i])) continue;
	if(exclude && sizeof(exclude)) {
	    x = sizeof(exclude);
	    while(x--)
		if(exclude[x] == inv[i]) break;
	    if(x >= 0) continue;
	}
	items += ({ inv[i] });
    }

    i = sizeof(items);

    if(!i) return "";

    list = ([]);

    while(i--) {

        tmp = items[i]->query_short();

        if(!stringp(tmp) || tmp == "") continue;

        if(!list[tmp]) list[tmp] = ({ items[i] });

        else list[tmp] += ({ items[i] });

    }

    i = sizeof(shorts = keys(list));

    if(!i) return "";

    if((x=sizeof(list[shorts[--i]])) == 1)

      ret = capitalize(shorts[i]);

    else ret = capitalize(consolidate(x, shorts[i]));

    if(!i) return (x <2 ? ret+" is here.\n" : ret +" are here.\n");

    else if(i==1)

      return ret+" and "+consolidate(sizeof(list[shorts[0]]), shorts[0])+

        " are here.\n";

    else {

        while(i--) {

            if(!i) ret += ", and ";

            else ret += ", ";

            ret += consolidate(sizeof(list[shorts[i]]), shorts[i]);

        }

    }

    return ret+" are here.";

}



int filter_living(object ob) { return living(ob); }

int filter_non_living(object ob) { return !living(ob); }



int query_closed() { return is_closed; }

