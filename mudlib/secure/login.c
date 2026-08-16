// Minimal login object for /secure/master until full Nightmare /adm/obj/login is wired.

void logon() {
    write("Welcome to RustMud.\n\n");
    write("Login (enter your handle): ");
    input_to("get_name");
}

void get_name(string str) {
    if (!str || str == "") {
        write("Invalid entry. Try again: ");
        input_to("get_name");
        return;
    }
    write("Hello, " + capitalize(str) + ".\n");
    write("Full character login is not wired yet; you are connected as a stub.\n");
    write("> ");
}

int process_input(string str) {
    if (!str) {
        return 1;
    }
    if (str == "quit" || str == "logout") {
        write("Goodbye.\n");
        return 0;
    }
    write("You said: " + str + "\n> ");
    return 1;
}
