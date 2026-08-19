/*    /daemon/command.c
 *    from Nightmare IV
 *    a new commands daemon, much faster than the old
 *    created by Descartes of Borg 940119
 *    Data storage concept by Archimedes@Nightmare
 */

#include <dirs.h>

private nosave mapping __Cmds;
private nosave string *__Paths;

void rehash(mixed *val);
void ensure_paths(string *path);
string find_cmd(string cmd, string *path);
string *query_paths();
varargs string *query_commands(string str);

void create() {
    seteuid(getuid());
    __Cmds = ([]);
    __Paths = ({});
    /* Index mortal/skills at boot; guild/hm/creator paths via ensure_paths at login. */
    rehash( ({ DIR_MORTAL_CMDS, DIR_CLASS_CMDS }) );
  }

string find_cmd(string cmd, string *path) {
    string *tmp;

    if(!cmd || !pointerp(path)) return 0;
    if(__Cmds[cmd] && sizeof(tmp = (path & (string *)__Cmds[cmd])))
      return sprintf("%s/_%s", tmp[0], cmd);
    return 0;
  }

void ensure_paths(string *path) {
    string *tmp;

    if(!pointerp(path)) return;
    tmp = path - (path & __Paths);
    if(sizeof(tmp)) rehash(tmp);
  }

void rehash(mixed val) {
    string *choses;
    int i, j;

    if(stringp(val)) val = ({ val });
    else if(!pointerp(val)) return;
    i = sizeof(val);
    while(i--) {
        debug_message("CMD_D rehash start " + val[i]);
        if(this_player())
            message("info", "Loading commands: " + val[i] + " ...", this_player());
        if(file_size(val[i]) ==-2) //check to see if it's a directory
		{
	        j = sizeof(choses = get_dir(val[i]+"/_*.c"));
        	while(j--) {
            			choses[j] = choses[j][1..strlen(choses[j])-3];
			        if(pointerp(__Cmds[choses[j]])) __Cmds[choses[j]] += ({ val[i] });
			           else __Cmds[choses[j]] = ({ val[i] });
			   }
                debug_message("CMD_D rehash done " + val[i] + " (" +
                  sizeof(choses) + " cmds)");
		}
        else
            debug_message("CMD_D rehash skip (not a dir) " + val[i]);
        if(this_player())
            message("info", "  done " + val[i], this_player());
        __Paths = distinct_array(__Paths + ({ val[i] }));
      }
  }

string *query_paths() { return __Paths; }

varargs string *query_commands(string str) {
    string *cmds, *tmp;
    int i;

    if(!str) return keys(__Cmds);
    i = sizeof(cmds = keys(__Cmds));
    tmp = ({});
    while(i--) if(member_array(str, __Cmds[cmds[i]]) != -1) tmp += ({cmds[i]});
    return tmp;
  }
