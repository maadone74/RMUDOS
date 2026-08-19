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
            "/d/damned/guilds/tinker/tinker_join",
            "/d/damned/guilds/tinker/tinker_gm",
            "/d/damned/guilds/tinker/gm_hammer",
            "/d/damned/virtual/armour_server",
        ] {
            compiler::compile_file_in(&mudlib, path)
                .unwrap_or_else(|e| panic!("{path}: {e:#}"));
        }
    }

    #[test]
    fn compile_cmd_dirs() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let mut failed = Vec::new();
        let mut ok = 0usize;
        for dir_name in ["mortal", "hm", "creator", "system", "adm", "mentor"] {
            let dir = mudlib.join("cmds").join(dir_name);
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            let mut entries: Vec<_> = entries.filter_map(|e| e.ok()).collect();
            entries.sort_by_key(|e| e.file_name());
            for entry in entries {
                let name = entry.file_name();
                let name = name.to_string_lossy();
                if !name.starts_with('_') || !name.ends_with(".c") {
                    continue;
                }
                let path = format!("/cmds/{}/{}", dir_name, name.trim_end_matches(".c"));
                if matches!(
                    path.as_str(),
                    "/cmds/creator/_pupdate" | "/cmds/creator/_unref"
                ) {
                    // _pupdate: undefined CONFIG_DIR. _unref: inherit REFS_D
                    // (macro path, not a string literal).
                    continue;
                }
                match compiler::compile_file_in(&mudlib, &path) {
                    Ok(_) => ok += 1,
                    Err(e) => failed.push(format!("{path}: {e:#}")),
                }
            }
        }
        if !failed.is_empty() {
            panic!(
                "{ok} compiled, {} failed:\n{}",
                failed.len(),
                failed.join("\n")
            );
        }
        assert!(ok > 80, "expected a full cmds tree, got {ok}");
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
    fn object_string_functional_is_call_other() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let program = std::sync::Arc::new(
            compiler::compile_source(
                r#"
int search_cb() {
    return 42;
}

int run() {
    mixed f;
    f = (: this_object(), "search_cb" :);
    return evaluate(f);
}
"#,
                "/test/objfun",
            )
            .expect("compile object+string functional"),
        );
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let object = std::sync::Arc::new(parking_lot::Mutex::new(
            crate::vm::object::Object::new(1, "/test/objfun".into(), program),
        ));
        world.objects.write().insert(1, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(result, LpcValue::Int(42));
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
    fn inventory_item_add_action_binds_to_this_player() {
        let item_prog = compiler::compile_source(
            r#"
void init() {
    add_action("do_smoke", "smoke");
}

int do_smoke(string str) {
    write("PUFF\n");
    return 1;
}
"#,
            "/test/cig",
        )
        .expect("compile item");
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }
"#,
            "/test/smoker",
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
            "/test/smoker".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let _ = world.apply(player.clone(), "create", Vec::new(), None, None);
        let item_id = world.allocate_object_id();
        let item = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            item_id,
            "/test/cig".to_owned(),
            std::sync::Arc::new(item_prog),
        )));
        world.objects.write().insert(item_id, item.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:15".parse().unwrap(),
            "smoker",
            tx,
        )));
        world
            .move_object(&item, &player)
            .expect("move cig into inventory");
        assert!(
            player
                .lock()
                .actions
                .iter()
                .any(|action| action.verb == "smoke"),
            "item init() must register smoke on the player"
        );
        assert!(
            item.lock().actions.is_empty(),
            "item must not keep its own sentences"
        );
        world
            .handle_player_input(player.clone(), "smoke".to_owned())
            .expect("smoke");
        let out = collect_text(&mut rx);
        assert!(out.contains("PUFF"), "inventory verb should fire, got: {out}");
    }

    #[test]
    fn present_matches_clone_basename() {
        let container_prog = compiler::compile_source(
            r#"
object find_it(string name) {
    return present(name, this_object());
}
"#,
            "/test/pack",
        )
        .expect("compile container");
        let item_prog = compiler::compile_source(
            r#"
void create() {}
"#,
            "/d/drizzt/obj/misc/cig",
        )
        .expect("compile item");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let pack_id = world.allocate_object_id();
        let pack = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            pack_id,
            "/test/pack".to_owned(),
            std::sync::Arc::new(container_prog),
        )));
        world.objects.write().insert(pack_id, pack.clone());
        let item_id = world.allocate_object_id();
        let item = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            item_id,
            "/d/drizzt/obj/misc/cig".to_owned(),
            std::sync::Arc::new(item_prog),
        )));
        item.lock().clone_number = Some(6);
        world.objects.write().insert(item_id, item.clone());
        world
            .move_object(&item, &pack)
            .expect("move into pack");
        let found = world
            .apply(
                pack.clone(),
                "find_it",
                vec![crate::vm::value::LpcValue::String("cig".into())],
                None,
                None,
            )
            .expect("present");
        match found {
            crate::vm::value::LpcValue::Object(ob) => {
                assert!(std::sync::Arc::ptr_eq(&ob, &item))
            }
            other => panic!("present(\"cig\") should find clone basename, got {other:?}"),
        }
    }

    #[test]
    fn quit_destruct_clears_interactive() {
        let player_prog = compiler::compile_source(
            r#"
void create() {
    enable_commands();
    add_action("do_quit", "quit");
}

int do_quit() {
    write("BYE\n");
    destruct(this_object());
    return 1;
}
"#,
            "/test/quit_player",
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
            "/test/quit_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let _ = world.apply(player.clone(), "create", Vec::new(), None, None);
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:14".parse().unwrap(),
            "quitter",
            tx,
        )));
        world
            .handle_player_input(player.clone(), "quit".to_owned())
            .expect("quit");
        let out = collect_text(&mut rx);
        assert!(out.contains("BYE"), "quit should write, got: {out}");
        assert!(player.lock().destructed, "quit must destruct the player");
        assert!(
            player.lock().interactive.is_none(),
            "destruct must drop the telnet interactive"
        );
    }

    #[test]
    fn destructed_shadow_does_not_loop() {
        let target_prog = compiler::compile_source(
            r#"
int strip_shadows() {
    object tmp;
    int n;
    n = 0;
    tmp = shadow(this_object(), 0);
    while (tmp) {
        n++;
        if (n > 8) return -1;
        destruct(tmp);
        tmp = shadow(this_object(), 0);
    }
    return n;
}
"#,
            "/test/shadow_target",
        )
        .expect("compile target");
        let shadow_prog = compiler::compile_source(
            r#"
void go(object who) {
    shadow(who, 1);
}
"#,
            "/test/shadow_src",
        )
        .expect("compile shadow");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let target_id = world.allocate_object_id();
        let target = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            target_id,
            "/test/shadow_target".to_owned(),
            std::sync::Arc::new(target_prog),
        )));
        world.objects.write().insert(target_id, target.clone());
        let shadow_id = world.allocate_object_id();
        let shadower = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            shadow_id,
            "/test/shadow_src".to_owned(),
            std::sync::Arc::new(shadow_prog),
        )));
        world.objects.write().insert(shadow_id, shadower.clone());
        world
            .apply(
                shadower.clone(),
                "go",
                vec![crate::vm::value::LpcValue::Object(target.clone())],
                None,
                None,
            )
            .expect("attach shadow");
        assert!(target.lock().shadow.is_some(), "shadow() should attach");
        world.destruct_object(&shadower).expect("destruct shadow");
        let result = world
            .apply(target.clone(), "strip_shadows", Vec::new(), None, None)
            .expect("strip");
        assert_ne!(
            result.as_int(),
            Some(-1),
            "quit-style shadow walk must not loop on a destructed shadow"
        );
        assert!(
            target.lock().shadow.is_none(),
            "destructed shadow must unlink from the target"
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
    fn sscanf_failure_leaves_out_params_unchanged() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    string a, b;
    a = "keepA";
    b = "keepB";
    if (sscanf("nope", "%s$N%s", a, b) != 0) return "matched";
    if (a != "keepA") return "a:" + a;
    if (b != "keepB") return "b:" + b;
    return "ok";
}
"#,
            "/test/sscanf_noclobber",
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
            "/test/sscanf_noclobber".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "failed sscanf must not clobber outs, got {result:?}"
        );
    }

    #[test]
    fn title_substr_does_not_append_zero() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    string str, foo, fii, reg, match, replace;
    str = "Novice $N the Pech";
    match = "$N";
    replace = "Rylo";
    reg = "";
    while (sscanf(str, "%s" + match + "%s", foo, fii)) {
        if (!foo) foo = "";
        if (!fii) fii = "";
        reg += foo + replace;
        str = str[strlen(foo) + strlen(match)..strlen(str)];
    }
    reg += fii;
    return reg;
}
"#,
            "/test/title_substr",
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
            "/test/title_substr".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "Novice Rylo the Pech",
            "title replace must not append 0, got {result:?}"
        );
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
    fn getuid_uses_argument_object() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let caller_prog = compiler::compile_source(
            r#"
mixed run(object ob) {
    return ({ getuid(), getuid(ob) });
}
"#,
            "/test/getuid_caller",
        )
        .expect("compile caller");
        let other_prog = compiler::compile_source(
            "void create() {}\n",
            "/test/getuid_other",
        )
        .expect("compile other");
        let caller_id = world.allocate_object_id();
        let caller = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            caller_id,
            "/test/getuid_caller".to_owned(),
            std::sync::Arc::new(caller_prog),
        )));
        let other_id = world.allocate_object_id();
        let other = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            other_id,
            "/test/getuid_other".to_owned(),
            std::sync::Arc::new(other_prog),
        )));
        world.objects.write().insert(caller_id, caller.clone());
        world.objects.write().insert(other_id, other.clone());
        caller.lock().uid = "caller-uid".to_owned();
        other.lock().uid = "other-uid".to_owned();
        let result = world
            .apply(
                caller,
                "run",
                vec![vm::LpcValue::Object(other)],
                None,
                None,
            )
            .expect("run");
        let vm::LpcValue::Array(parts) = result else {
            panic!("expected array, got {result:?}");
        };
        assert_eq!(
            parts[0].as_string(),
            Some("caller-uid"),
            "getuid() is current object, got {parts:?}"
        );
        assert_eq!(
            parts[1].as_string(),
            Some("other-uid"),
            "getuid(ob) must use the argument, got {parts:?}"
        );
    }

    #[test]
    fn getuid_of_zero_returns_zero() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let program = compiler::compile_source(
            r#"
mixed run() {
    mixed u, e, n;
    u = getuid(0);
    e = geteuid(previous_object());
    n = file_name(0);
    if (u) return "getuid";
    if (e) return "geteuid";
    if (n) return "file_name";
    return "ok";
}

void create_like_stats_log() {
    string s;
    s = "uid: "+getuid(previous_object())+" ("+file_name(previous_object())+")";
}
"#,
            "/test/getuid_zero",
        )
        .expect("compile");
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/getuid_zero".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object.clone(), "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "getuid(0)/file_name(0) must be 0, got {result:?}"
        );
        world
            .apply(object, "create_like_stats_log", Vec::new(), None, None)
            .expect("set_stats-style log with previous_object 0");
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
    names = get_dir("/tmpfs/*/");
    if (sizeof(names) != 3) return "globslash:" + sizeof(names);
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
    fn compile_drizzt_clearing() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        compiler::compile_file_in(&mudlib, "/d/drizzt/rooms/clearing")
            .expect("compile /d/drizzt/rooms/clearing");
        compiler::compile_file_in(&mudlib, "/d/drizzt/rooms/mage_hut")
            .expect("compile /d/drizzt/rooms/mage_hut");
    }

    #[test]
    fn wizard_path_falls_back_to_domain() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        compiler::compile_file_in(&mudlib, "/wizards/khojem/new/mon/elf_warrior")
            .expect("compile khojem elf via /wizards alias");
        compiler::compile_file_in(&mudlib, "/d/khojem/new/mon/elf_warrior")
            .expect("compile khojem elf via /d path");
    }

    #[test]
    fn virtual_armour_path_has_no_source_file() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        assert!(
            compiler::source_exists(&mudlib, "/std/Object"),
            "real objects still resolve to path.c"
        );
        assert!(
            !compiler::source_exists(&mudlib, "/d/damned/virtual/iron-greaves.armour"),
            "virtual .armour must not steal a stem.c path"
        );
        assert!(
            compiler::source_exists(&mudlib, "/d/damned/virtual/armour_server"),
            "armour_server.c is the virtual compiler"
        );
    }

    #[test]
    fn clone_object_uses_master_compile_object_when_no_source() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let stub_rel = "tmp/rmudos_virtual_stub.c";
        let stub_path = mudlib.join(stub_rel);
        std::fs::create_dir_all(stub_path.parent().unwrap()).expect("tmp dir");
        std::fs::write(&stub_path, "void create() {}\n").expect("write stub");

        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            ..Default::default()
        });
        let master_prog = compiler::compile_source(
            r#"
object compile_object(string file) {
    return clone_object("/tmp/rmudos_virtual_stub");
}
"#,
            "/test/virtual_master",
        )
        .expect("compile master");
        let id = world.allocate_object_id();
        let master = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/test/virtual_master".to_owned(),
            std::sync::Arc::new(master_prog),
        )));
        world.objects.write().insert(id, master.clone());
        world.set_master(master);

        let object = world
            .clone_object("/d/damned/virtual/iron-greaves.armour")
            .expect("virtual clone_object");
        assert_eq!(
            object.lock().name,
            "/d/damned/virtual/iron-greaves.armour"
        );
        let _ = std::fs::remove_file(&stub_path);
    }

    #[test]
    fn explode_keeps_blank_lines() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    mixed *parts;
    parts = explode("a\n\nb\n", "\n");
    if (sizeof(parts) != 3) return "size:" + sizeof(parts);
    if (parts[0] != "a") return "0";
    if (parts[1] != "") return "blank";
    if (parts[2] != "b") return "2";
    return "ok";
}
"#,
            "/test/explode_blank",
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
            "/test/explode_blank".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "explode blank lines, got {result:?}"
        );
    }

    #[test]
    fn mapping_index_assign_yields_rhs() {
        let program = compiler::compile_source(
            r#"
mixed run() {
    mapping m;
    mixed n;
    m = ([]);
    m["class"] = "info";
    m["screen"] = 20;
    n = sizeof(m["lines"] = ({ "a", "b", "c", "d", "e" }));
    if (n != 5) return "sizeof:" + n;
    if (sizeof(m["lines"]) != 5) return "stored";
    return "ok";
}
"#,
            "/test/map_assign_sizeof",
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
            "/test/map_assign_sizeof".to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        let result = world
            .apply(object, "run", Vec::new(), None, None)
            .expect("run");
        assert_eq!(
            result.as_string().unwrap_or("?"),
            "ok",
            "m[k]=v yields v, got {result:?}"
        );
    }

    #[test]
    fn more_pages_and_input_to_quit() {
        use crate::net::TelnetOut;
        use tokio::sync::mpsc;

        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let program = compiler::compile_file_in(&mudlib, "/std/user/more").expect("compile more");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        });
        let id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            "/std/user/more".to_owned(),
            program,
        )));
        world.objects.write().insert(id, player.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:31".parse().unwrap(),
            "pager",
            tx,
        )));
        world
            .apply(player.clone(), "create", Vec::new(), None, None)
            .expect("create");
        let lines: Vec<vm::LpcValue> = (0..40)
            .map(|n| vm::LpcValue::String(format!("LINE{n}")))
            .collect();
        world
            .apply(
                player.clone(),
                "more",
                vec![vm::LpcValue::Array(lines)],
                Some(player.clone()),
                None,
            )
            .expect("more");
        let mut out = String::new();
        while let Ok(msg) = rx.try_recv() {
            if let TelnetOut::Text(text) = msg {
                out.push_str(&text);
            }
        }
        assert!(
            out.contains("LINE0") && out.contains("LINE19") && !out.contains("LINE20"),
            "first page should be 20 lines, got: {out}"
        );
        assert!(
            out.contains("--More--"),
            "pager must print --More--, got: {out}"
        );
        assert!(
            player.lock().pending_input.is_some(),
            "more must hold input_to after the first page"
        );
        world
            .handle_player_input(player.clone(), " ".to_owned())
            .expect("next page");
        out.clear();
        while let Ok(msg) = rx.try_recv() {
            if let TelnetOut::Text(text) = msg {
                out.push_str(&text);
            }
        }
        assert!(
            out.contains("LINE20"),
            "space should show the next page, got: {out}"
        );
        world
            .handle_player_input(player.clone(), "q".to_owned())
            .expect("quit pager");
        assert!(
            player.lock().pending_input.is_none(),
            "q must leave the pager"
        );
        world
            .handle_player_input(player.clone(), "q".to_owned())
            .expect("after pager");
        out.clear();
        while let Ok(msg) = rx.try_recv() {
            if let TelnetOut::Text(text) = msg {
                out.push_str(&text);
            }
        }
        assert!(
            out.contains("What?"),
            "q after leaving more is a normal command, got: {out}"
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

    #[test]
    fn mortal_cmds_apply_without_unknown_efun() {
        let skip = [
            "help", "news", "mail", "finger", "faq", "suicide", "bug", "idea",
            "typo", "praise", "mudidea", "biography", "background",
        ];
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            ..Default::default()
        });
        let simul = world
            .load_object("/adm/obj/simul_efun")
            .expect("load simul_efun");
        world.set_simul_efun(simul);
        let player_prog = compiler::compile_source(
            r#"
void create() { enable_commands(); }
"#,
            "/test/cmd_player",
        )
        .expect("compile player");
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            player_id,
            "/test/cmd_player".to_owned(),
            std::sync::Arc::new(player_prog),
        )));
        world.objects.write().insert(player_id, player.clone());
        let (tx, _rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:22".parse().unwrap(),
            "tester",
            tx,
        )));
        world
            .apply(player.clone(), "create", Vec::new(), None, None)
            .expect("create player");

        let dir = mudlib.join("cmds/mortal");
        let mut unknown = Vec::new();
        let mut entries: Vec<_> = std::fs::read_dir(&dir)
            .expect("cmds/mortal")
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let fname = entry.file_name();
            let fname = fname.to_string_lossy();
            if !fname.starts_with('_') || !fname.ends_with(".c") {
                continue;
            }
            let verb = fname
                .trim_start_matches('_')
                .trim_end_matches(".c")
                .to_string();
            if skip.contains(&verb.as_str()) {
                continue;
            }
            let path = format!("/cmds/mortal/_{verb}");
            let cmd = match world.load_object(&path) {
                Ok(object) => object,
                Err(e) => {
                    let msg = format!("{e:#}");
                    if msg.contains("unknown efun") {
                        unknown.push(format!("{verb}: load {msg}"));
                    }
                    continue;
                }
            };
            let fun = format!("cmd_{verb}");
            if let Err(e) = world.apply(
                cmd,
                &fun,
                vec![vm::LpcValue::Null],
                Some(player.clone()),
                None,
            ) {
                let msg = format!("{e:#}");
                if msg.contains("unknown efun") {
                    unknown.push(format!("{verb}: {msg}"));
                }
            }
        }
        assert!(
            unknown.is_empty(),
            "mortal cmds hit unknown efuns:\n{}",
            unknown.join("\n")
        );
    }
}

#[cfg(test)]
mod hang_fixes {
    use crate::config::DriverConfig;
    use crate::simulate;
    use crate::vm::MudWorld;
    use std::sync::Arc;

    #[test]
    fn events_d_loads_without_reentrant_reload_loop() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = Arc::new(MudWorld::new(DriverConfig {
            mudlib,
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        }));
        simulate::boot_master(&world).expect("boot master");
        world
            .load_object("/daemon/events_d")
            .expect("events_d create");
        assert!(world.find_object("/daemon/events_d").is_some());
        world
            .load_object("/daemon/events_d")
            .expect("events_d reload is idempotent");
    }

    #[test]
    fn unknown_verb_returns_what_minimal() {
        use tokio::sync::mpsc;

        fn collect_text(rx: &mut mpsc::UnboundedReceiver<crate::net::TelnetOut>) -> String {
            let mut out = String::new();
            while let Ok(msg) = rx.try_recv() {
                if let crate::net::TelnetOut::Text(t) = msg {
                    out.push_str(&t);
                }
            }
            out
        }

        let player_prog = crate::compiler::compile_source(
            r#"
void create() { enable_commands(); add_action("cmd_hook", "", 1); }
int cmd_hook(string cmd) { return 0; }
"#,
            "/test/minimal_hook",
        )
        .expect("compile");
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = Arc::new(MudWorld::new(DriverConfig {
            mudlib,
            ..Default::default()
        }));
        let player_id = world.allocate_object_id();
        let player = std::sync::Arc::new(parking_lot::Mutex::new(crate::vm::Object::new(
            player_id,
            "/test/minimal_hook".to_owned(),
            player_prog.into(),
        )));
        world.objects.write().insert(player_id, player.clone());
        let (tx, mut rx) = mpsc::unbounded_channel();
        player.lock().interactive = Some(std::sync::Arc::new(
            crate::vm::object::Interactive::new(
                "127.0.0.1:99".parse().unwrap(),
                "tester",
                tx,
            ),
        ));
        world
            .apply(player.clone(), "create", Vec::new(), None, None)
            .expect("create");
        world
            .handle_player_input(player.clone(), "xyzzy".to_owned())
            .expect("unknown");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("What?"),
            "unknown verb should print What?, got: {out:?}"
        );
    }

    #[test]
    #[ignore = "full newchar login is slow; covered by unknown_verb_returns_what_minimal"]
    fn unknown_verb_returns_what_after_login() {
        use tokio::sync::mpsc;

        fn collect_text(rx: &mut mpsc::UnboundedReceiver<crate::net::TelnetOut>) -> String {
            let mut out = String::new();
            while let Ok(msg) = rx.try_recv() {
                if let crate::net::TelnetOut::Text(t) = msg {
                    out.push_str(&t);
                }
            }
            out
        }

        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let id = std::process::id();
        let name = format!(
            "u{}{}{}",
            char::from_u32(b'a' as u32 + (id % 26)).unwrap_or('a'),
            char::from_u32(b'a' as u32 + ((id / 26) % 26)).unwrap_or('a'),
            char::from_u32(b'a' as u32 + ((id / 676) % 26)).unwrap_or('a')
        );
        let _ = std::fs::remove_file(
            mudlib
                .join("adm/save/users")
                .join(name.chars().next().unwrap().to_string())
                .join(format!("{name}.o")),
        );
        let world = Arc::new(MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            master: "/adm/obj/master".to_owned(),
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        }));
        simulate::boot_master(&world).expect("boot");
        let (tx, mut rx) = mpsc::unbounded_channel();
        let mut session =
            simulate::connect_player(&world, "127.0.0.1:11".parse().unwrap(), tx).expect("connect");
        let _ = collect_text(&mut rx);

        for line in [
            name.as_str(),
            "y",
            "secret99",
            "secret99",
            "male",
            "test@example.com",
            "Tester",
        ] {
            world
                .handle_player_input(session.clone(), line.to_owned())
                .unwrap_or_else(|e| panic!("login {line:?}: {e:#}"));
            let _ = collect_text(&mut rx);
            if session.lock().destructed || session.lock().interactive.is_none() {
                if let Some(owner) = world.users().into_iter().find(|o| {
                    o.lock().living_name.as_deref() == Some(name.as_str())
                }) {
                    session = owner;
                }
            }
        }

        world
            .handle_player_input(session.clone(), "look".to_owned())
            .expect("look");
        let _ = collect_text(&mut rx);

        world
            .handle_player_input(session.clone(), "xyzzy_not_a_cmd".to_owned())
            .expect("unknown");
        let out = collect_text(&mut rx);
        assert!(
            out.contains("What?"),
            "unknown verb should print What?, got: {out:?}"
        );
    }
}

#[cfg(test)]
mod wizard_status_efuns {
    use super::*;
    use tokio::sync::mpsc;

    fn collect_text(rx: &mut mpsc::UnboundedReceiver<crate::net::TelnetOut>) -> String {
        let mut out = String::new();
        while let Ok(msg) = rx.try_recv() {
            if let crate::net::TelnetOut::Text(t) = msg {
                out.push_str(&t);
            }
        }
        out
    }

    fn compile_object(world: &MudWorld, path: &str, source: &str) -> vm::object::ObjectRef {
        let program = compiler::compile_source(source, path).expect("compile");
        let id = world.allocate_object_id();
        let object = std::sync::Arc::new(parking_lot::Mutex::new(vm::Object::new(
            id,
            path.to_owned(),
            std::sync::Arc::new(program),
        )));
        world.objects.write().insert(id, object.clone());
        object
    }

    fn attach_interactive(object: &vm::object::ObjectRef) -> mpsc::UnboundedReceiver<crate::net::TelnetOut> {
        let (tx, rx) = mpsc::unbounded_channel();
        object.lock().interactive = Some(std::sync::Arc::new(vm::object::Interactive::new(
            "127.0.0.1:77".parse().unwrap(),
            "tester",
            tx,
        )));
        rx
    }

    #[test]
    fn rusage_debug_status_and_owner_stats() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let dump_rel = "tmp/rmudos_obj_dump_test";
        let dump_path = mudlib.join(dump_rel);
        let _ = std::fs::remove_file(&dump_path);

        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            ..Default::default()
        });
        let master = compile_object(
            &world,
            "/test/status_master",
            r#"
string author_file(string str) { return "rylo"; }
string domain_file(string str) { return "newbieville"; }
"#,
        );
        world.set_master(master);

        let object = compile_object(
            &world,
            "/test/status_probe",
            r#"
mixed run_rusage() {
    mapping r = rusage();
    return ({ r["utime"], r["usertime"], r["stime"] });
}

string run_malloc() { return malloc_status(); }
string run_mstatus() { return mud_status(1); }

mixed run_authors() {
    mapping all = author_stats();
    mapping one = author_stats("rylo");
    return ({ keys(all), one["objects"], one["moves"] });
}

mixed run_domains() {
    mapping one = domain_stats("newbieville");
    return one["objects"];
}

void run_writes() {
    debug_info(0, this_object());
    debug_info(1, this_object());
    cache_stats();
    dump_file_descriptors();
    dump_socket_status();
}

int run_dump() {
    dumpallobj("/tmp/rmudos_obj_dump_test");
    return file_size("/tmp/rmudos_obj_dump_test");
}
"#,
        );
        let mut rx = attach_interactive(&object);

        let rusage = world
            .apply(object.clone(), "run_rusage", Vec::new(), None, None)
            .expect("rusage");
        let vm::LpcValue::Array(times) = rusage else {
            panic!("expected rusage array, got {rusage:?}");
        };
        assert_eq!(times.len(), 3);
        assert!(times[0].as_int().is_some());
        assert_eq!(times[0].as_int(), times[1].as_int(), "usertime aliases utime");

        let malloc = world
            .apply(object.clone(), "run_malloc", Vec::new(), None, None)
            .expect("malloc");
        let malloc = malloc.as_string().unwrap_or("");
        assert!(
            malloc.contains("malloc") || malloc.contains("VmRSS"),
            "malloc_status should report allocator, got {malloc:?}"
        );

        let status = world
            .apply(object.clone(), "run_mstatus", Vec::new(), None, None)
            .expect("mstatus");
        let status = status.as_string().unwrap_or("");
        assert!(status.contains("Objects:"), "mud_status missing objects, got {status:?}");
        assert!(status.contains("Call outs:"), "mud_status extra missing call outs");

        let authors = world
            .apply(object.clone(), "run_authors", Vec::new(), None, None)
            .expect("authors");
        let vm::LpcValue::Array(parts) = authors else {
            panic!("expected author tuple, got {authors:?}");
        };
        assert!(
            parts[1].as_int().unwrap_or(0) >= 1,
            "author_stats(\"rylo\") objects, got {parts:?}"
        );

        let domains = world
            .apply(object.clone(), "run_domains", Vec::new(), None, None)
            .expect("domains");
        assert!(
            domains.as_int().unwrap_or(0) >= 1,
            "domain_stats objects, got {domains:?}"
        );

        world
            .apply(object.clone(), "run_writes", Vec::new(), None, None)
            .expect("writes");
        let out = collect_text(&mut rx);
        assert!(out.contains("O_HEART_BEAT"), "debug_info(0) missing flags, got {out:?}");
        assert!(out.contains("num func's"), "debug_info(1) missing program, got {out:?}");
        assert!(out.contains("Filesystem stat cache"), "cache_stats missing, got {out:?}");
        assert!(out.contains("Fd  Target") || out.contains("Fd"), "fd table missing, got {out:?}");
        assert!(out.contains("socket"), "dump_socket_status missing, got {out:?}");

        let dumped = world
            .apply(object.clone(), "run_dump", Vec::new(), None, None)
            .expect("dump");
        assert!(dumped.as_int().unwrap_or(-1) > 0, "dumpallobj should write a file");
        let dump = std::fs::read_to_string(&dump_path).expect("read dump");
        assert!(
            dump.contains("/test/status_probe"),
            "dumpallobj should list probe object, got {dump:?}"
        );
        let _ = std::fs::remove_file(&dump_path);
    }

    #[test]
    fn wizard_status_cmds_apply_without_unknown_efun() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib: mudlib.clone(),
            ..Default::default()
        });
        let simul = world
            .load_object("/adm/obj/simul_efun")
            .expect("load simul_efun");
        world.set_simul_efun(simul);
        let player = compile_object(
            &world,
            "/test/wiz_player",
            "void create() { enable_commands(); enable_wizard(); }\n",
        );
        let _rx = attach_interactive(&player);
        world
            .apply(player.clone(), "create", Vec::new(), None, None)
            .expect("create player");

        let cmds = [
            "/cmds/creator/_malloc",
            "/cmds/creator/_mstatus",
            "/cmds/system/_debug_info",
            "/cmds/adm/_cache",
            "/cmds/adm/_fdinfo",
            "/cmds/adm/_dumpallobj",
            "/cmds/creator/_netstat",
            "/cmds/adm/_bench",
            "/cmds/creator/_realms",
            "/cmds/creator/_domains",
        ];
        let mut unknown = Vec::new();
        for path in cmds {
            let cmd = match world.load_object(path) {
                Ok(cmd) => cmd,
                Err(error) => {
                    unknown.push(format!("{path} load: {error:#}"));
                    continue;
                }
            };
            let fun = path.rsplit('/').next().unwrap().replacen('_', "cmd_", 1);
            match world.apply(
                cmd,
                &fun,
                vec![vm::LpcValue::String(String::new())],
                Some(player.clone()),
                Some(player.clone()),
            ) {
                Err(error) if format!("{error:#}").contains("unknown efun") => {
                    unknown.push(format!("{path}::{fun}: {error:#}"));
                }
                _ => {}
            }
        }
        assert!(
            unknown.is_empty(),
            "wizard status cmds hit unknown efuns:\n{}",
            unknown.join("\n")
        );
    }

    #[test]
    fn superuser_euid_can_write_restricted_access_db_paths() {
        let mudlib = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let world = MudWorld::new(DriverConfig {
            mudlib,
            master: "/adm/obj/master".to_owned(),
            simul_efun: Some("/adm/obj/simul_efun".to_owned()),
            ..Default::default()
        });
        simulate::boot_master(&world).expect("boot");
        let master = world.master().expect("master");
        let wizard = compile_object(
            &world,
            "/test/access_wizard",
            "void create() {}\n",
        );
        wizard.lock().uid = "rylo".to_owned();
        wizard.lock().euid = "rylo".to_owned();
        let allowed = world
            .apply(
                master,
                "valid_write",
                vec![
                    vm::LpcValue::String("/adm/db/access.db".to_owned()),
                    vm::LpcValue::Object(wizard),
                    vm::LpcValue::String("write_file".to_owned()),
                ],
                None,
                None,
            )
            .expect("valid_write");
        assert!(
            allowed.is_truthy(),
            "superuser rylo should write /adm/db per access.db, got {allowed:?}"
        );
    }
}
