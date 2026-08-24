
inherit "/tmp/rmudos_inheritc";
void create() {
    ::add_money(42);
}
int run() { return query_money(); }
