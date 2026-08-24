//      /daemon/mudlib/economy_d.c
//      from the Nightmare Mudlib
//      a daemon to handle currenciy inflation
//      created by Descartes of Borg 931114

#include <security.h>
#include <save.h>
#include <clock.h>

private mapping Currencies;
int LastInflation;

private void ensure_default_currencies();

void create() {
    string *borg;
    float temps, tmp;
    int i;

    Currencies = ([]);
    seteuid(UID_DAEMONSAVE);
    if(catch(restore_object(SAVE_ECONOMY))) {
      rm("/daemon/save/economy.o");
      cp("/daemon/save/economy.bak", "/daemon/save/economy.o");
      restore_object(SAVE_ECONOMY);
    }
    /* Fresh / incomplete checkouts often ship an empty Currencies ([])
     * save. Without rates, shops list everything as "0 copper". */
    ensure_default_currencies();
    i = sizeof(borg = keys(Currencies));
    temps = percent(time()-LastInflation, YEAR)* 0.01;
    while(i--) { 
        tmp = temps * Currencies[borg[i]]["inflation"];
        if(intp(Currencies[borg[i]]["rate"]))
          Currencies[borg[i]]["rate"] = to_float(Currencies[borg[i]]["rate"]);
        Currencies[borg[i]]["rate"] += tmp*Currencies[borg[i]]["rate"];
    }
    LastInflation = time();
    seteuid(UID_DAEMONSAVE);
    save_object(SAVE_ECONOMY);
    seteuid(getuid());
}

private void ensure_default_currencies() {
    if(mapp(Currencies) && sizeof(keys(Currencies))) return;
    /* Rates multiply object query_value() (gold units) into that currency.
     * Doc: 1 platinum = 10 gold = 100 electrum = 500 silver = 1000 copper */
    Currencies = ([
      "mithril": ([ "rate":0.01, "inflation":0.02, "weight":0.05 ]),
      "platinum": ([ "rate":0.1, "inflation":0.02, "weight":0.05 ]),
      "gold": ([ "rate":1.0, "inflation":0.03, "weight":0.1 ]),
      "electrum": ([ "rate":10.0, "inflation":0.03, "weight":0.1 ]),
      "silver": ([ "rate":50.0, "inflation":0.04, "weight":0.1 ]),
      "copper": ([ "rate":1000.0, "inflation":0.05, "weight":0.15 ]),
    ]);
    LastInflation = time();
}

void add_currency(string type, float rate, float infl, float wt) {
    if(geteuid(previous_object()) != UID_APPROVAL) return;
    if(!mapp(Currencies)) Currencies = ([]);
    if(!type || !rate || !infl || !wt || Currencies[type]) return;
    if(intp(rate)) rate = to_float(rate);
    Currencies[type] = ([ "rate":rate, "inflation":infl, "weight":wt ]);
    seteuid(UID_DAEMONSAVE);
    save_object(SAVE_ECONOMY);
    seteuid(getuid());
}

void change_currency(string type, string key, float x) {
    if(geteuid(previous_object()) != UID_APPROVAL) return;
    if(!mapp(Currencies)) Currencies = ([]);
    if(!type || !Currencies[type] || !key || !x) return;
    if(!Currencies[type][key]) return;
    Currencies[type][key] = x;
    seteuid(UID_DAEMONSAVE);
    save_object(SAVE_ECONOMY);
    seteuid(getuid());
}

float __Query(string type, string key) {
    if(!mapp(Currencies) || undefinedp(Currencies[type]) ||
       !mapp(Currencies[type]))
	return 0.0;
    return Currencies[type][key]; 
}

string *__QueryCurrencies() { return keys(Currencies); }
