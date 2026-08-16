//    /adm/simul_efun/overrides.c
//    from the Nightmare Mudlib
//    efun overriding code
//    created by someone for Basis

#include <security.h>
#include <objects.h>
#include <commands.h>

//   Restores v0.9.20 behavior of member_array.
//   Diewarzau 4/16/96

varargs int member_array(mixed item, mixed *arr, int flag) {
    int len, i;
    mixed *arr2;

    if(!arr || !item) return -1;
    if(flag && stringp(item)) {
      len = strlen(item);
      i = sizeof(arr);
      arr2 = allocate(sizeof(arr));
      len--;
      while(i--) {
      if(stringp(arr[i]) && strlen(arr[i]) > (len+1)) {
        arr2[i] = arr[i][0..len];
      }
    else arr2[i] = arr[i];
    }
    return efun::member_array(item, arr2);
  }
  return efun::member_array(item, arr);
}

void destruct(object destructee) {
    string destee, dester;
    object prev;

    if(!destructee) return;
    prev = previous_object();
    destee = geteuid(destructee);
    dester = prev ? geteuid(prev) : 0;
    /* Login must be able to discard unfinished /std/user clones (bad email,
     * aborted creation, etc.) even if its euid was not yet Root. */
    if(prev && base_name(prev) == OB_LOGIN) {
	efun::destruct(destructee);
	return;
    }
    if(!destee || dester==destee || dester==UID_ROOT ||
      dester==UID_DESTRUCT ||
      master()->query_member_group(dester, "superuser") ||
      master()->query_member_group(dester, "assist"))
	efun::destruct(destructee);
    else if(dester && owner_euid(destee) == dester)
	efun::destruct(destructee);
    else write(capitalize(dester ? dester : "Someone")+" is not allowed to destruct "+
	  capitalize(destee ? destee : file_name(destructee))+"\n");
}

#if 0
varargs void shutdown(int code) {
    if(geteuid(previous_object()) != UID_ROOT &&
      geteuid(previous_object()) != UID_SHUTDOWN) return;
    log_file("shutdowns", "Game shutdown by "+
      (this_player() ? capitalize(geteuid(this_player())) : "Root")+
      " at "+ctime(time())+"\n");
    seteuid(UID_SHUTDOWN);
    catch(call_other("/daemon/nameserver_d", "send_shutdown"));
    seteuid(getuid());
    efun::shutdown(code);
}
#endif


varargs object snoop(object snooper, object snoopee) {
    if(!snoopee) return efun::snoop(snooper);
    if(!wizardp(snooper)) return 0;
    if((int)master()->query_member_group(geteuid(snooper), "superuser") || 
      (int)master()->query_member_group(geteuid(snooper), "assist") ||
      snoopee->query_snoop()) {
	if((int)master()->query_member_group(geteuid(snoopee), "superuser"))
	    tell_object(snoopee, capitalize((string)snooper->query_name())+
	      " is now snooping you.\n");
	return efun::snoop(snooper, snoopee);
    }
    return 0;
}

int exec(object to_obj, object from_obj) {
    object prev;

    if(!to_obj || !from_obj) return 0;
    /* Login must be able to transfer its socket onto /std/user. */
    if(base_name(from_obj) == "/adm/obj/login")
	return efun::exec(to_obj, from_obj);
    prev = previous_object();
    if(prev && geteuid(prev) == UID_ROOT)
	return efun::exec(to_obj, from_obj);
    return 0;
}

object query_snoop(object snoopee) {
    if(geteuid(previous_object()) != UID_SNOOP &&
      base_name(previous_object()) != OB_USER) return 0;
    return efun::query_snoop(snoopee);
}

object query_snooping(object ob) {
    if(geteuid(previous_object()) != UID_ROOT) return 0;
    return efun::query_snooping(ob);
}

void write(string str) {
    object dest;
    // After exec(), this_player is often still the login object (no socket).
    // Prefer an interactive destination so setup/room text reaches the player.
    if(this_object() && interactive(this_object())) dest = this_object();
    else if(this_player() && interactive(this_player())) dest = this_player();
    else if(this_player()) dest = this_player();
    else {
	efun::write(str);
	return;
    }
    message("my_action", str, dest);
}            

string *deep_inherit_list(object ob) {
  return map_array(efun::deep_inherit_list(ob), (: replace_string :),
    "/", "", 1);
}

//   Allows message to be supplied with 0 as a third arg, in
//    which case it does nothing.  A compat. fixer.
//    Diewarzau 4/23/96

varargs void message(string type, string mess, mixed to, mixed exclude) {
  if(!to) return;
  efun::message(type, mess, to, exclude);
  return;
}
