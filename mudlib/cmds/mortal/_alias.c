/*
// alias command
*/
 
#include <std.h>
 
inherit DAEMON;
 
void alias_reset ();
 
varargs int cmd_alias (string str, int first)
{
  int i, sl;
  int index;
  string verb,cmd,*elements;
  object act_ob;
  mapping alias;
  
  act_ob = previous_object();
  if(str == "-clear")
    {
      act_ob->clear_aliases();
      return 1;
    }
  if(str == "-reset")
    {
      alias_reset();
      return 1;
    }
  alias = (mapping) act_ob->query_aliases();
  if(!str)
    {
      if (!mapp(alias)) {
          if(pointerp(alias)) write("Your alias mapping is a pointer!");
          if(intp(alias)) write("You alias mapping is an integer!");
          if(stringp(alias)) write ("Your alias mapping is a string!");
          if(alias == 0) write ("You alias mapping is 0!");
          notify_fail("You have some bad aliases, dude!");
          return 0;
      }
      elements = keys(alias);
      if(!elements || !sizeof(elements))
       {
         write("No aliases defined.");
         return 1;
       }
      for(i = 0; i < sizeof(elements); i+=2)
      {
       write(arrange_string(elements[i], 9) +
           arrange_string(alias[elements[i]], 60));
       if(i+1 < sizeof(elements))
           write(arrange_string(elements[i+1], 9) +
               arrange_string(alias[elements[i+1]], 60));
       }
      return 1;
    }
  
  if(sscanf(str,"%s %s",verb,cmd) == 2)
    {
      if(verb=="alias")
       {
         notify_fail ("Sorry, you can't alias 'alias'.\n");
         return 0;
       }
      if (!first)
       if (!alias[verb])
         write("Alias: "+verb+" ("+cmd+") added.");
       else
         write("Alias: "+verb+" ("+cmd+") altered.");
      act_ob->add_alias(verb,cmd);
      return 1;
    }
  if(!alias[str])
    {
      write("The alias "+str+" wasn't found.");
      return 1;
    }
  printf("%-15s%s\n",str,alias[str]);
  return 1;
}
 
 
varargs void alias_reset()
{
  
  if (!interactive(previous_object()))
    return;
  cmd_alias("exa look at $*",1);
  cmd_alias("i inventory",1);
  cmd_alias("l look $*",1);
  cmd_alias("$' say $*",1);
  cmd_alias("$\" say $*",1);
  cmd_alias("bio biography",1);
  cmd_alias("$: emote $*",1);
  cmd_alias("e east");
  cmd_alias("w west");
  cmd_alias("n north");
  cmd_alias("s south");
  cmd_alias("u up");
  cmd_alias("d down");
  cmd_alias("ne northeast");
  cmd_alias("nw northwest");
  cmd_alias("sw southwest");
  cmd_alias("se southeast");
}
 
 
int help()
{
  write("Flags accepted:\n");
  write("    -reset\tClears aliases and sets them to defaults.\n");
  write("    -clear\tClears all aliases.\n");
  
  write("\nalias\t\t\tView current aliases.\n");
  write("alias <alias> <command>\tSet the verb alias to execute command.\n");
  write("alias <alias>\t\tCheck the value of <alias>\n");
  write("unalias <alias>\t\tRemove <alias> from the alias list.\n");
  
  write("\nSubstitution variables:\n");
  write("    $N - Word N after the verb ($1 = first arg, $2 = second, ...).\n");
  write("    $* - All args after the verb.  If the alias also uses $N, $*\n");
  write("         is only the remaining words after the highest $N used.\n");
  write("         Example: alias shopbuy say shopkeeper, offer $1 mithril "
	"for $*\n");
  write("         Then: shopbuy 1000 storage locker\n");
  write("         Becomes: say shopkeeper, offer 1000 mithril for "
	"storage locker\n");
  write("\nPrefix the alias verb with $ for a no-space prefix verb.\n");
  write("Example: alias $' say $*   then type: 'Hey there!\n");
  write("\nLook at the default aliases for examples.\n");
  write("See also: unalias, nickname, unnickname, history\n");
  return 1;
}
