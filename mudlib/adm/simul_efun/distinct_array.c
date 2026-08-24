//    /adm/simul_efun/distinct_array.c
//    from Nightmare IV
//    a faster, better namedfunction based on Huthar's uniq_array()
//    by Descartes of Borg 940117
//
//    RMUDOS: do not use objects as mapping keys.  The driver stores mapping
//    keys as strings, so keys() would return path strings and combat's
//    distinct_array(query_wielded()) would lose the weapon objects
//    (to-hit type=0 / skill -24).  Equality scan preserves MudOS results.

mixed *distinct_array(mixed *arr) {
    mixed *ret;
    int i, j, maxi, dup;

    if (!pointerp(arr)) return ({});
    ret = ({});
    maxi = sizeof(arr);
    for (i = 0; i < maxi; i++) {
	dup = 0;
	for (j = 0; j < sizeof(ret); j++) {
	    if (ret[j] == arr[i]) {
		dup = 1;
		break;
	    }
	}
	if (!dup) ret += ({ arr[i] });
    }
    return ret;
}
