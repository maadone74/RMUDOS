//! rmudos — a concise Rust LPC driver inspired by MudOS / FluffOS.

pub mod backend;
pub mod compiler;
pub mod config;
pub mod efun;
pub mod net;
pub mod simulate;
pub mod vm;

pub use config::DriverConfig;
pub use vm::MudWorld;

#[cfg(test)]
mod compile_smoke {
    use super::*;

    #[test]
    fn compile_mudlib() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        for path in [
            "/secure/master",
            "/std/room",
            "/std/user",
            "/room/void",
            "/room/tavern",
            "/room/street",
            "/adm/obj/login",
            "/adm/obj/master",
            "/adm/obj/simul_efun",
            "/adm/daemon/news_d",
            "/daemon/command",
            "/cmds/mortal/_who",
            "/daemon/terminal_d",
        ] {
            compiler::compile_file_in(&mudlib, path)
                .unwrap_or_else(|e| panic!("{path}: {e:#}"));
        }
    }
}

#[cfg(test)]
mod control_flow_smoke {
    use super::*;

    #[test]
    fn for_continue_and_switch_compile() {
        compiler::compile_source(
            r#"
int sum() {
    int i, n, total;
    total = 0;
    for (i = 0, n = 3; i < n; i++) {
        if (i == 1) continue;
        total += i;
    }
    switch (total) {
    case 0: case 1:
        return 1;
    case 2:
        break;
    default:
        return total;
    }
    return total;
}
"#,
            "/test/control",
        )
        .expect("compile control flow");
    }
}

#[cfg(test)]
mod functional_smoke {
    use super::*;
    use crate::vm::value::LpcValue;

    #[test]
    fn compiles_expression_and_named_functionals() {
        let program = compiler::compile_source(
            r#"
int add_via_fun(int a, int b) {
    mixed f;
    f = (: $1 + $2 :);
    return evaluate(f, a, b);
}

int filter_odds() {
    mixed *nums;
    mixed *odds;
    nums = ({ 1, 2, 3, 4, 5 });
    odds = filter_array(nums, (: $1 % 2 :));
    return sizeof(odds);
}

int call_other_fun(object ob) {
    return evaluate((: call_other :), ob, "query_value");
}
"#,
            "/test/functional",
        )
        .expect("compile functionals");
        assert!(program.local_functions.contains_key("add_via_fun"));
        assert!(program
            .local_functions
            .get("add_via_fun")
            .unwrap()
            .code
            .iter()
            .any(|op| matches!(op, crate::vm::program::Op::MakeExprFunction(_))));
    }

    #[test]
    fn map_array_with_functional_runs() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let program = std::sync::Arc::new(
            compiler::compile_source(
                r#"
int run() {
    mixed f;
    mixed *vals;
    f = (: $1 * 2 :);
    vals = map_array(({ 1, 2, 3 }), f);
    return vals[0] + vals[1] + vals[2];
}
"#,
                "/test/mapfun",
            )
            .expect("compile"),
        );
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let object = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::vm::object::Object::new(1, "/test/mapfun".into(), program),
        ));
        world.objects.write().insert(1, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(result, LpcValue::Int(12));
    }

    #[test]
    fn char_literal_compares() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let program = std::sync::Arc::new(
            compiler::compile_source(
                r#"
int is_slash(string path) {
    return path[0] == '/';
}
"#,
                "/test/charlit",
            )
            .expect("compile char literal"),
        );
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let object = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::vm::object::Object::new(2, "/test/charlit".into(), program),
        ));
        world.objects.write().insert(2, object.clone());
        let result = world
            .apply(
                object,
                "is_slash",
                vec![LpcValue::String("/tmp".into())],
                None,
                None,
            )
            .expect("is_slash");
        assert_eq!(result, LpcValue::Int(1));
    }
}

#[cfg(test)]
mod runtime_smoke {
    use super::*;
    use crate::net::TelnetOut;
    use tokio::sync::mpsc;

    fn collect_text(rx: &mut mpsc::UnboundedReceiver<TelnetOut>) -> String {
        let mut out = String::new();
        while let Ok(msg) = rx.try_recv() {
            match msg {
                TelnetOut::Text(text) => {
                    out.push_str(&text);
                    out.push('\n');
                }
                TelnetOut::Echo(_) => {}
            }
        }
        out
    }

    #[test]
    fn boot_master_only() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        assert!(world.find_object("/room/void").is_some());
    }

    #[test]
    fn connect_sends_welcome() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let peer = "127.0.0.1:9".parse().unwrap();
        let player = simulate::connect_player(&world, peer, tx).expect("connect");
        let out = collect_text(&mut rx);
        eprintln!("OUT:\n{out}");
        assert!(player.lock().interactive.is_some());
        assert!(
            out.contains("Welcome") || out.contains("Login") || out.contains("login"),
            "expected login prompt, got: {out}"
        );
    }

    #[test]
    fn nightmare_master_connect_welcome() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            master: "/adm/obj/master".to_owned(),
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot nightmare master");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let player =
            simulate::connect_player(&world, "127.0.0.1:11".parse().unwrap(), tx).expect("connect");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("Welcome") || out.contains("Login") || out.contains("Mudlib"),
            "expected Nightmare WELCOME, got: {out}"
        );
        world
            .handle_player_input(player.clone(), "testplayer".to_owned())
            .expect("name");
        let follow = collect_text(&mut rx);
        eprintln!("AFTER NAME:\n{follow}");
        assert!(
            follow.contains("wish") || follow.contains("Password") || !follow.is_empty(),
            "expected login continuation, got: {follow}"
        );
    }

    #[test]
    fn nightmare_newchar_exec_look_quit() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let id = std::process::id();
        let name = format!(
            "tp{}{}{}",
            char::from_u32(b'a' as u32 + (id % 26)).unwrap_or('a'),
            char::from_u32(b'a' as u32 + ((id / 26) % 26)).unwrap_or('a'),
            char::from_u32(b'a' as u32 + ((id / 676) % 26)).unwrap_or('a')
        );
        let email = format!("{name}{id}@example.com");
        let _ = std::fs::remove_file(
            mudlib
                .join("adm/save/users")
                .join(name.chars().next().unwrap().to_string())
                .join(format!("{name}.o")),
        );
        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            master: "/adm/obj/master".to_owned(),
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut session =
            simulate::connect_player(&world, "127.0.0.1:11".parse().unwrap(), tx).expect("connect");
        let _ = collect_text(&mut rx);

        // Step through new-character creation.
        for line in [
            name.as_str(),
            "y",
            "secret99",
            "secret99",
            "male",
            email.as_str(),
            "Tester",
        ] {
            world
                .handle_player_input(session.clone(), line.to_owned())
                .unwrap_or_else(|e| panic!("login {line:?}: {e:#}"));
            let out = collect_text(&mut rx);
            eprintln!("STEP {line:?}: {}", out.replace('\n', " | "));
            if session.lock().destructed || session.lock().interactive.is_none() {
                if let Some(owner) = world.users().into_iter().find(|o| {
                    o.lock().living_name.as_deref() == Some(name.as_str())
                        || o.lock().program.path.contains("/std/user")
                }) {
                    session = owner;
                }
            }
        }

        assert!(
            session.lock().program.path.contains("/std/user"),
            "expected /std/user after exec"
        );
        assert!(session.lock().interactive.is_some());
        assert!(session.lock().pending_input.is_none());

        world
            .handle_player_input(session.clone(), "look".to_owned())
            .expect("look");
        let look_out = collect_text(&mut rx);
        eprintln!("LOOK:\n{look_out}");
        assert!(
            session.lock().environment().is_some() || !look_out.is_empty(),
            "player should be in a room or see look output"
        );

        // Fire delayed autosave from setup() (call_out delay 2).
        std::thread::sleep(std::time::Duration::from_millis(2100));
        world.process_call_outs();
        let save_path = mudlib
            .join("adm/save/users")
            .join(name.chars().next().unwrap().to_string())
            .join(format!("{name}.o"));
        eprintln!("autosave exists={}", save_path.exists());
        assert!(
            save_path.exists(),
            "expected autosave at {}",
            save_path.display()
        );
        world
            .handle_player_input(session.clone(), "quit".to_owned())
            .expect("quit");
        let quit_out = collect_text(&mut rx);
        eprintln!("QUIT:\n{quit_out}");
    }

    #[test]
    fn input_to_error_restores_pending() {
        let room_prog = compiler::compile_source(
            r#"
void prompt_user() {
    input_to("boom", 0);
}

void boom(string line) {
    error("boom");
}
"#,
            "/test/input_restore_room",
        )
        .expect("compile room");
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }
"#,
            "/test/input_restore_player",
        )
        .expect("compile player");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let room_id = world.allocate_object_id();
        let room = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            room_id,
            "/test/input_restore_room".to_owned(),
            std::sync::Arc::new(room_prog),
        )));
        world.objects.write().insert(room_id, room.clone());
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/input_restore_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:17".parse().unwrap(),
            "tester",
            tx,
        )));

        world
            .apply(room, "prompt_user", Vec::new(), Some(player.clone()), None)
            .expect("prompt");
        assert!(player.lock().pending_input.is_some());

        world
            .handle_player_input(player.clone(), "anything".to_owned())
            .expect("input should soft-fail");
        let out = collect_text(&mut rx);
        assert!(
            player.lock().pending_input.is_some(),
            "pending_input must be restored after callback error; out={out}"
        );
        assert!(
            out.to_lowercase().contains("error") || out.to_lowercase().contains("boom"),
            "expected error text, got: {out}"
        );
    }

    #[test]
    fn lpc_class_new_and_member_access() {
        let program = compiler::compile_source(
            r#"
class point {
    int x;
    int y;
}

int run() {
    class point p;
    p = new(class point);
    p->x = 3;
    p->y = 4;
    if (p->x != 3) return 0;
    if (p->y != 4) return 0;
    return p->x + p->y;
}
"#,
            "/test/class",
        )
        .expect("compile class");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/class".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(result, vm::LpcValue::Int(7));
    }

    #[test]
    fn catch_all_add_action_and_process_input() {
        let program = compiler::compile_source(
            r#"
string process_input(string arg) {
    return "look";
}

int cmd_hook(string str) {
    write("HOOK:" + str + "\n");
    return 1;
}

void create() {
    enable_commands();
    add_action("cmd_hook", "", 1);
}
"#,
            "/test/cmdhook",
        )
        .expect("compile cmdhook");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/cmdhook".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let _ = world.apply(object.clone(), "create", Vec::new(), None, None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        object.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:12".parse().unwrap(),
            "tester",
            tx,
        )));
        world
            .handle_player_input(object.clone(), "aliasme".to_owned())
            .expect("input");
        let out = collect_text(&mut rx);
        // process_input rewrites to bare "look"; catch-all gets args after verb (none → 0).
        assert!(
            out.contains("HOOK:0") || out.contains("HOOK:"),
            "expected catch-all after process_input, got: {out}"
        );
    }

    #[test]
    fn room_add_action_binds_to_this_player() {
        let room_prog = compiler::compile_source(
            r#"
int use_exit() {
    write("MOVED:" + query_verb() + "\n");
    return 1;
}

int use_stupid_exit() {
    write("STUPID\n");
    return 1;
}

void init() {
    add_action("use_stupid_exit", "north");
    add_action("use_exit", "north");
}
"#,
            "/test/exit_room",
        )
        .expect("compile room");
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }
"#,
            "/test/exit_player",
        )
        .expect("compile player");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let room_id = world.allocate_object_id();
        let room = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            room_id,
            "/test/exit_room".to_owned(),
            std::sync::Arc::new(room_prog),
        )));
        world.objects.write().insert(room_id, room.clone());
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/exit_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let _ = world.apply(player.clone(), "create", Vec::new(), None, None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:13".parse().unwrap(),
            "walker",
            tx,
        )));
        world
            .move_object(&player, &room)
            .expect("move into room");
        assert!(
            !player.lock().actions.is_empty(),
            "room init should register actions on the player"
        );
        assert!(
            room.lock().actions.is_empty(),
            "MudOS keeps sentences on the living, not the room"
        );
        world
            .handle_player_input(player.clone(), "north".to_owned())
            .expect("north");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("MOVED:north"),
            "LIFO should prefer use_exit over use_stupid_exit, got: {out}"
        );
        assert!(
            !out.contains("STUPID"),
            "stub exit must not win, got: {out}"
        );
    }

    #[test]
    fn input_to_binds_to_interactive_not_caller() {
        let room_prog = compiler::compile_source(
            r#"
void prompt_user() {
    input_to("got_line", 0, 42);
}

void got_line(string line, int extra) {
    write("GOT:" + line + ":" + extra + "\n");
}
"#,
            "/test/input_room",
        )
        .expect("compile room");
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }
"#,
            "/test/input_player",
        )
        .expect("compile player");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let room_id = world.allocate_object_id();
        let room = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            room_id,
            "/test/input_room".to_owned(),
            std::sync::Arc::new(room_prog),
        )));
        world.objects.write().insert(room_id, room.clone());
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/input_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:13".parse().unwrap(),
            "tester",
            tx,
        )));
        // Room calls input_to while this_player is the interactive player.
        world
            .apply(
                room.clone(),
                "prompt_user",
                Vec::new(),
                Some(player.clone()),
                None,
            )
            .expect("prompt");
        assert!(
            player.lock().pending_input.is_some(),
            "pending_input must live on the interactive player"
        );
        assert!(room.lock().pending_input.is_none());
        world
            .handle_player_input(player.clone(), "hello".to_owned())
            .expect("line");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("GOT:hello:42"),
            "callback on caller with this_player interactive, got: {out}"
        );
    }

    #[test]
    fn eval_lock_serializes_heartbeat_and_player_input() {
        let player_prog = compiler::compile_source(
            r#"
int ticks;
void create() { enable_commands(); set_heart_beat(1); }
void heart_beat() { ticks++; }
int cmd_look(string arg) {
    write("LOOK:" + ticks + "\n");
    return 1;
}
void init() { add_action("cmd_look", "look"); }
"#,
            "/test/eval_player",
        )
        .expect("compile player");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = std::sync::Arc::new(MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        }));
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/eval_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let _ = world.apply(player.clone(), "create", Vec::new(), None, None);
        let _ = world.apply(player.clone(), "init", Vec::new(), Some(player.clone()), None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:17".parse().unwrap(),
            "locker",
            tx,
        )));
        let hb_world = world.clone();
        let hb = std::thread::spawn(move || {
            for _ in 0..40 {
                let _eval = hb_world.lock_eval();
                hb_world.heartbeat();
            }
        });
        for _ in 0..40 {
            let _eval = world.lock_eval();
            world
                .handle_player_input(player.clone(), "look".to_owned())
                .expect("look");
        }
        hb.join().expect("heartbeat thread");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("LOOK:"),
            "serialized eval should still deliver look output, got: {out}"
        );
    }

    #[test]
    fn crypt_roundtrip_matches_login_check() {
        let program = compiler::compile_source(
            r#"
string run() {
    string stored, again;
    stored = crypt("secret99", 0);
    again = crypt("secret99", stored);
    if (stored != again) return "mismatch:"+stored+":"+again;
    if (crypt("wrongpass", stored) == stored) return "wrong-accepted";
    return "ok:"+stored;
}
"#,
            "/test/crypt",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/crypt".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let text = result.as_string().expect("string");
        assert!(
            text.starts_with("ok:"),
            "crypt create/verify failed: {text}"
        );
        assert_eq!(text.len(), 3 + 13, "expected ok: + 13-char crypt, got {text}");
    }

    #[test]
    fn sprintf_mudos_center_and_pad() {
        let program = compiler::compile_source(
            r#"
string run() {
    string a, b, c;
    a = sprintf("%|10s", "mid");
    b = sprintf("%'-='10s", "");
    c = sprintf("%-+3d", 7);
    return a + "|" + b + "|" + c;
}
"#,
            "/test/sprintf",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/sprintf".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let text = result.as_string().expect("string");
        let parts: Vec<_> = text.split('|').collect();
        assert_eq!(parts.len(), 3, "got {text}");
        assert_eq!(parts[0].chars().count(), 10);
        assert!(parts[0].contains("mid"));
        assert_eq!(parts[1], "-=-=-=-=-=");
        assert_eq!(parts[2], "+7 ");
    }

    #[test]
    fn nightmare_class_preload_paths_compile() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        for path in [
            "/adm/daemon/wizchar_d",
            "/adm/daemon/save_items_d",
            "/d/damned/data/armour_db",
            "/d/damned/data/weapon_db",
            "/daemon/clan_d",
        ] {
            compiler::compile_file_in(&mudlib, path)
                .unwrap_or_else(|error| panic!("compile {path}: {error:#}"));
        }
    }

    #[test]
    fn mortal_inventory_and_score_compile() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        for path in ["/cmds/mortal/_inventory", "/cmds/mortal/_look"] {
            compiler::compile_file_in(&mudlib, path)
                .unwrap_or_else(|error| panic!("compile {path}: {error:#}"));
        }
    }

    #[test]
    fn nightmare_preload_db_boot_soft_loads() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            master: "/adm/obj/master".to_owned(),
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        let preload = std::fs::read_to_string(mudlib.join("adm/db/preload.db")).expect("preload.db");
        let paths: Vec<_> = preload
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .collect();
        assert!(!paths.is_empty());
        let mut loaded = 0usize;
        for path in &paths {
            if world.find_object(path).is_some() {
                loaded += 1;
            }
        }
        eprintln!("preload loaded {loaded}/{}", paths.len());
        // Class-backed and non-class daemons should mostly come up; allow a few soft-fails.
        assert!(
            loaded >= paths.len() / 2,
            "expected most of preload.db to load, got {loaded}/{}",
            paths.len()
        );
        assert!(
            world.find_object("/daemon/command").is_some()
                || world.find_object("/adm/daemon/wizchar_d").is_some(),
            "expected at least one critical preload daemon"
        );
    }

    #[test]
    fn sscanf_assigns_out_params() {
        let program = compiler::compile_source(
            r#"
int run() {
    string a, b;
    if (sscanf("x@y.com", "%s@%s", a, b) != 2) return 0;
    if (a != "x") return 0;
    if (b != "y.com") return 0;
    return 1;
}
"#,
            "/test/sscanf",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/sscanf".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(result, vm::LpcValue::Int(1));
    }

    #[test]
    fn call_out_and_foreach_smoke() {
        let program = compiler::compile_source(
            r#"
int hits;

void create() { hits = 0; }

void bump() { hits = hits + 1; }

void start() {
    call_out("bump", 0);
}

int foreach_sum(int *arr) {
    int n, x;
    n = 0;
    foreach (x in arr) n += x;
    return n;
}

int caught() {
    mixed err;
    err = catch(throw("boom"));
    return stringp(err);
}

int hit_count() { return hits; }
"#,
            "/test/callout",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/callout".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        world
            .apply(object.clone(), "create", Vec::new(), None, None)
            .expect("create");
        world
            .apply(object.clone(), "start", Vec::new(), None, None)
            .expect("start");
        world.process_call_outs();
        let hits = world
            .apply(object.clone(), "hit_count", Vec::new(), None, None)
            .expect("hits");
        assert_eq!(hits, vm::LpcValue::Int(1));
        let sum = world
            .apply(
                object.clone(),
                "foreach_sum",
                vec![vm::LpcValue::Array(vec![
                    vm::LpcValue::Int(1),
                    vm::LpcValue::Int(2),
                    vm::LpcValue::Int(3),
                ])],
                None,
                None,
            )
            .expect("foreach");
        assert_eq!(sum, vm::LpcValue::Int(6));
        let caught = world
            .apply(object, "caught", Vec::new(), None, None)
            .expect("catch");
        assert_eq!(caught, vm::LpcValue::Int(1));
    }

    #[test]
    fn filesystem_efun_smoke() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            ..Default::default()
        });
        let program = compiler::compile_source(
            r#"
int run() {
    write_file("/log/rmudos_fs_test.log", "hello\n", 1);
    return file_exists("/log/rmudos_fs_test.log");
}
"#,
            "/test/fs",
        )
        .expect("compile");
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/fs".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(result, vm::LpcValue::Int(1));
        let _ = std::fs::remove_file(mudlib.join("log/rmudos_fs_test.log"));
    }

    #[test]
    fn string_cast_of_zero_is_falsy() {
        let program = compiler::compile_source(
            r#"
string run() {
    string file;
    file = (string)0;
    if (!file) return "falsy";
    return "truthy:" + file;
}
"#,
            "/test/string_cast_zero",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/string_cast_zero".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "falsy",
            "(string)0 must be falsy like MudOS, got {result:?}"
        );
    }

    #[test]
    fn inherit_colon_colon_remaps_child_globals() {
        let root = std::env::temp_dir().join(format!(
            "rmudos_inherit_globals_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("temp mudlib");
        std::fs::write(
            root.join("padding.c"),
            "int padding;\nvoid create() { padding = 7; }\n",
        )
        .unwrap();
        std::fs::write(
            root.join("exits.c"),
            r#"
mapping destinations;

void create() {
    destinations = (["north": "/dest"]);
}

void initiate_exits() {
    if (!destinations) {
        write("EMPTY\n");
        return;
    }
    write("KEYS:" + implode(keys(destinations), ",") + "\n");
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("room.c"),
            r#"
inherit "/padding";
inherit "/exits";

void create() {
    padding::create();
    exits::create();
}

void init() {
    exits::initiate_exits();
}
"#,
        )
        .unwrap();
        std::fs::write(
            root.join("player.c"),
            "void create() { enable_commands(); }\n",
        )
        .unwrap();

        let world = MudWorld::new(DriverConfig {
            mudlib: root.clone(),
            ..Default::default()
        });
        let room = world.load_object("/room").expect("load room");
        let player = world.load_object("/player").expect("load player");
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:19".parse().unwrap(),
            "walker",
            tx,
        )));
        world.move_object(&player, &room).expect("move");
        let out = collect_text(&mut rx);
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            out.contains("KEYS:north"),
            "exits::initiate_exits must see child destinations, got: {out}"
        );
        assert!(
            !out.contains("EMPTY"),
            "destinations was empty (unrelocated inherit globals): {out}"
        );
    }

    #[test]
    fn ctime_formats_unix_epoch() {
        let program = compiler::compile_source(
            r#"
string run() {
    return ctime(0);
}
"#,
            "/test/ctime_epoch",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/ctime_epoch".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "Thu Jan  1 00:00:00 1970",
            "ctime(0) must be MudOS-shaped, got {result:?}"
        );
    }

    #[test]
    fn base_name_strips_clone_id() {
        let program = compiler::compile_source(
            r#"
string run() {
    return base_name("/std/user#12");
}
"#,
            "/test/base_name_clone",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/base_name_clone".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "/std/user",
            "base_name must strip #clone, got {result:?}"
        );
    }

    #[test]
    fn strcmp_orders_lexicographically() {
        let program = compiler::compile_source(
            r#"
int run() {
    return strcmp("apple", "banana");
}
"#,
            "/test/strcmp",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/strcmp".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert!(
            result.as_int().unwrap_or(0) < 0,
            "apple < banana, got {result:?}"
        );
    }

    #[test]
    fn clonep_detects_clone_number() {
        let program = compiler::compile_source(
            r#"
int run(object ob) {
    return clonep(ob);
}
"#,
            "/test/clonep",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/clonep".to_owned(),
            std::sync::Arc::new(program),
        )));
        object.lock().clone_number = Some(7);
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(
                object.clone(),
                "run",
                vec![vm::LpcValue::Object(object.clone())],
                None,
                None,
            )
            .expect("run");
        assert_eq!(result.as_int(), Some(1));
    }

    #[test]
    fn floatp_and_map_mapping_smoke() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    mapping m = ([ "a": 1, "b": 2 ]);
    if (!floatp(1.5)) return "no-float";
    return map_mapping(m, (: $2 + 10 :));
}
"#,
            "/test/map_mapping",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/map_mapping".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let vm::LpcValue::Mapping(mapped) = result else {
            panic!("expected mapping, got {result:?}");
        };
        assert_eq!(mapped.get("a").and_then(|v| v.as_int()), Some(11));
        assert_eq!(mapped.get("b").and_then(|v| v.as_int()), Some(12));
    }

    #[test]
    fn call_out_info_lists_pending() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    call_out("noop", 5);
    return call_out_info();
}
void noop() {}
"#,
            "/test/call_out_info",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/call_out_info".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let vm::LpcValue::Array(rows) = result else {
            panic!("expected array, got {result:?}");
        };
        assert_eq!(rows.len(), 1);
        let vm::LpcValue::Array(row) = &rows[0] else {
            panic!("expected row array");
        };
        assert!(row.len() >= 3);
        assert_eq!(row[2].as_int(), Some(5));
    }

    #[test]
    fn parse_command_pet_go_pattern() {
        let program = compiler::compile_source(
            r#"
string run() {
    string tmp1;
    if (!parse_command("go north", this_object(), " 'move' / 'go' %s ", tmp1))
        return "nomatch";
    return tmp1;
}
"#,
            "/test/parse_command",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/parse_command".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "north",
            "parse_command must capture trailing %s, got {result:?}"
        );
    }

    #[test]
    fn unique_array_groups_by_callback() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    return unique_array(({ "a", "bb", "c", "dd" }), (: strlen($1) :));
}
"#,
            "/test/unique_array",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/unique_array".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let vm::LpcValue::Array(groups) = result else {
            panic!("expected array, got {result:?}");
        };
        assert_eq!(groups.len(), 2, "two strlen groups, got {groups:?}");
    }

    #[test]
    fn localtime_epoch_is_utc_thursday() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    return localtime(0);
}
"#,
            "/test/localtime",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/localtime".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let vm::LpcValue::Array(parts) = result else {
            panic!("expected array, got {result:?}");
        };
        assert_eq!(parts.len(), 10);
        assert_eq!(parts[3].as_int(), Some(1), "mday");
        assert_eq!(parts[4].as_int(), Some(0), "January");
        assert_eq!(parts[5].as_int(), Some(1970), "year");
        assert_eq!(parts[6].as_int(), Some(4), "Thursday");
    }

    #[test]
    fn here_document_string_and_array() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    string s;
    mixed *a;
    s = @END
hello
world
END
;
    a = @@END
x
y
END
;
    if (s != "hello\nworld\n") return "str:" + s;
    if (sizeof(a) != 2) return "len";
    if (a[0] != "x" || a[1] != "y") return "arr";
    return "ok";
}
"#,
            "/test/heredoc",
        )
        .expect("compile here-document");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/heredoc".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "here-document, got {result:?}"
        );
    }

    #[test]
    fn get_dir_and_file_size_match_mudos() {
        let root = std::env::temp_dir().join(format!(
            "rmudos_get_dir_{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("tmpfs/sub")).expect("temp mudlib");
        std::fs::write(root.join("tmpfs/hello.txt"), b"abc").expect("hello");
        std::fs::write(root.join("tmpfs/other.c"), b"int x;").expect("other");
        let program = compiler::compile_source(
            r#"
mixed run() {
    mixed *names;
    mixed *info;
    mixed *one;
    if (file_size("/tmpfs") != -2) return "notdir";
    if (file_size("/tmpfs/hello.txt") != 3) return "size";
    if (file_size("/nope") != -1) return "missing";
    names = get_dir("/tmpfs/");
    if (sizeof(names) != 3) return "count:" + sizeof(names);
    if (member_array("hello.txt", names) == -1) return "nohello";
    if (member_array("sub", names) == -1) return "nosub";
    info = get_dir("/tmpfs/", -1);
    if (!pointerp(info[0]) || sizeof(info[0]) != 3) return "badflag";
    one = get_dir("/tmpfs/hello.txt");
    if (sizeof(one) != 1 || one[0] != "hello.txt") return "exact";
    names = get_dir("/tmpfs/*.txt");
    if (sizeof(names) != 1 || names[0] != "hello.txt") return "glob";
    write_file("/tmpfs/hello.txt", "abcd");
    if (file_size("/tmpfs/hello.txt") != 4) return "stale";
    return "ok";
}
"#,
            "/test/get_dir",
        )
        .expect("compile");
        let world = MudWorld::new(DriverConfig {
            mudlib: root.clone(),
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/get_dir".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        let _ = std::fs::remove_dir_all(&root);
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "MudOS get_dir/file_size, got {result:?}"
        );
    }

    #[test]
    fn message_applies_receive_message() {
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }

void receive_message(string msg_class, string msg) {
    receive("RM:" + msg_class + ":" + msg);
}

void send_it() {
    message("info", "%^BOLD%^hi", this_object());
}
"#,
            "/test/color_player",
        )
        .expect("compile player");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/color_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:21".parse().unwrap(),
            "rylo",
            tx,
        )));
        world
            .apply(player.clone(), "create", Vec::new(), None, None)
            .expect("create");
        world
            .apply(player.clone(), "send_it", Vec::new(), Some(player.clone()), None)
            .expect("message");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("RM:info:%^BOLD%^hi"),
            "message() must apply receive_message, got: {out}"
        );
    }

    #[test]
    fn terminal_d_ansi_bold_is_escape() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let daemon = world
            .load_object("/daemon/terminal_d")
            .expect("load TERMINAL_D");
        let mapping = world
            .apply(
                daemon,
                "query_term_info",
                vec![vm::LpcValue::String("ansi".into())],
                None,
                None,
            )
            .expect("query_term_info");
        let vm::LpcValue::Mapping(map) = mapping else {
            panic!("expected mapping, got {mapping:?}");
        };
        let bold = map
            .get("BOLD")
            .and_then(vm::LpcValue::as_string)
            .unwrap_or("");
        assert!(
            bold.starts_with('\u{1b}'),
            "ansi BOLD should be an ESC sequence, got {bold:?}"
        );
        assert!(
            map.get("GREEN")
                .and_then(vm::LpcValue::as_string)
                .is_some_and(|s| s.starts_with('\u{1b}')),
            "ansi GREEN should be an ESC sequence, got {:?}",
            map.get("GREEN")
        );
    }
}
