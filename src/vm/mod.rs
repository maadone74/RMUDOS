pub mod apply;
pub mod call_out;
pub mod interpret;
pub mod object;
pub mod program;
pub mod value;

pub use interpret::Interpreter;
pub use object::{Interactive, Object, ObjectId, ObjectRef, PendingInput};
pub use value::LpcValue;

use crate::compiler;
use crate::config::{normalize_object_path, DriverConfig};
use crate::efun::EfunTable;
use anyhow::{Context, Result};
use call_out::CallOutQueue;
use indexmap::IndexMap;
use object::{Object as ObjectStruct, ObjectId as Oid, ObjectRef as ORef};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

pub struct MudWorld {
    pub config: DriverConfig,
    pub efuns: EfunTable,
    pub(crate) objects: RwLock<IndexMap<Oid, ORef>>,
    pub(crate) blueprints: RwLock<HashMap<String, Oid>>,
    pub(crate) master: RwLock<Option<ORef>>,
    pub(crate) simul_efun: RwLock<Option<ORef>>,
    pub(crate) next_object_id: AtomicU64,
    pub(crate) next_clone_id: AtomicU64,
    pub shutdown_requested: AtomicBool,
    pub call_outs: CallOutQueue,
    /// Living name → object registry for `set_living_name` / `find_living`.
    pub(crate) livings: RwLock<HashMap<String, ORef>>,
}

impl MudWorld {
    pub fn new(config: DriverConfig) -> Self {
        Self {
            config,
            efuns: EfunTable::new(),
            objects: RwLock::new(IndexMap::new()),
            blueprints: RwLock::new(HashMap::new()),
            master: RwLock::new(None),
            simul_efun: RwLock::new(None),
            next_object_id: AtomicU64::new(1),
            next_clone_id: AtomicU64::new(1),
            shutdown_requested: AtomicBool::new(false),
            call_outs: CallOutQueue::new(),
            livings: RwLock::new(HashMap::new()),
        }
    }

    pub fn object(&self, id: Oid) -> Option<ORef> {
        self.objects.read().get(&id).cloned()
    }

    pub fn find_object(&self, path: &str) -> Option<ORef> {
        let path = normalize_object_path(path);
        let id = self.blueprints.read().get(&path).copied()?;
        self.object(id).filter(|object| !object.lock().destructed)
    }

    pub fn master(&self) -> Option<ORef> {
        self.master.read().clone()
    }

    pub fn set_master(&self, object: ORef) {
        *self.master.write() = Some(object);
    }

    pub fn simul_efun(&self) -> Option<ORef> {
        self.simul_efun.read().clone()
    }

    pub fn set_simul_efun(&self, object: ORef) {
        *self.simul_efun.write() = Some(object);
    }

    pub fn all_objects(&self) -> Vec<ORef> {
        self.objects
            .read()
            .values()
            .filter(|object| !object.lock().destructed)
            .cloned()
            .collect()
    }

    pub fn users(&self) -> Vec<ORef> {
        self.objects
            .read()
            .values()
            .filter(|object| {
                let object = object.lock();
                !object.destructed && object.interactive.is_some()
            })
            .cloned()
            .collect()
    }

    pub fn apply(
        &self,
        object: ORef,
        function: &str,
        arguments: Vec<value::LpcValue>,
        this_player: Option<ORef>,
        previous_object: Option<ORef>,
    ) -> Result<value::LpcValue> {
        thread_local! {
            static APPLY_DEPTH: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
            static APPLY_STACK: std::cell::RefCell<Vec<String>> = const { std::cell::RefCell::new(Vec::new()) };
        }

        let target = {
            let shadow = object.lock().shadow.clone();
            if let Some(shadow) = shadow {
                if !shadow.lock().destructed {
                    let program = shadow.lock().program.clone();
                    if Interpreter::find_function(&program, function).is_some() {
                        shadow
                    } else {
                        object.clone()
                    }
                } else {
                    object.clone()
                }
            } else {
                object.clone()
            }
        };

        let path = target.lock().name.clone();
        let frame = format!("{path}::{function}");
        let depth = APPLY_DEPTH.with(|d| {
            let next = d.get() + 1;
            d.set(next);
            next
        });
        APPLY_STACK.with(|s| s.borrow_mut().push(frame.clone()));
        // Soft guard against runaway apply nesting (each apply builds a new Interpreter).
        if depth > 256 {
            let stack = APPLY_STACK.with(|s| s.borrow().clone());
            APPLY_STACK.with(|s| {
                s.borrow_mut().pop();
            });
            APPLY_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
            anyhow::bail!(
                "apply recursion depth exceeded at {frame}\n{}",
                stack
                    .iter()
                    .rev()
                    .take(20)
                    .cloned()
                    .collect::<Vec<_>>()
                    .join("\n")
            );
        }

        let result = (|| {
            let mut interpreter =
                Interpreter::new(self, target, this_player, previous_object, self.config.max_cost);
            // Missing applies are soft-noops for optional hooks.
            let program = interpreter.current_object.lock().program.clone();
            if Interpreter::find_function(&program, function).is_none() {
                return Ok(value::LpcValue::Null);
            }
            interpreter.apply(function, arguments)
        })();

        APPLY_STACK.with(|s| {
            s.borrow_mut().pop();
        });
        APPLY_DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        result
    }

    pub fn load_object(&self, path: &str) -> Result<ORef> {
        let path = normalize_object_path(path);
        if let Some(existing) = self.find_object(&path) {
            return Ok(existing);
        }
        let program = match compiler::compile_file_in(&self.config.mudlib, &path) {
            Ok(program) => program,
            Err(compile_error) => {
                // Virtual objects via master->compile_object(path)
                if let Some(master) = self.master() {
                    match self.apply(
                        master,
                        "compile_object",
                        vec![value::LpcValue::String(path.clone())],
                        None,
                        None,
                    ) {
                        Ok(value::LpcValue::Object(object)) => return Ok(object),
                        Ok(value::LpcValue::String(resolved)) => {
                            return self.load_object(&resolved);
                        }
                        _ => {}
                    }
                }
                return Err(compile_error).with_context(|| format!("compile {path}"));
            }
        };
        let id = self.allocate_object_id();
        let object = Arc::new(Mutex::new(ObjectStruct::new(id, path.clone(), program)));
        self.objects.write().insert(id, object.clone());
        self.blueprints.write().insert(path.clone(), id);
        self.apply(object.clone(), "create", Vec::new(), None, None)
            .with_context(|| format!("create() in {path}"))?;
        Ok(object)
    }

    pub fn clone_object(&self, path: &str) -> Result<ORef> {
        let path = normalize_object_path(path);
        let program = compiler::compile_file_in(&self.config.mudlib, &path)?;
        let id = self.allocate_object_id();
        let clone_number = self.allocate_clone_id();
        let mut object = ObjectStruct::new(id, path.clone(), program);
        object.clone_number = Some(clone_number);
        let object = Arc::new(Mutex::new(object));
        self.objects.write().insert(id, object.clone());
        self.apply(object.clone(), "create", Vec::new(), None, None)
            .with_context(|| format!("create() in clone of {path}"))?;
        Ok(object)
    }

    pub fn destruct_object(&self, object: &ORef) -> Result<()> {
        self.call_outs.remove_object(object);
        {
            let mut livings = self.livings.write();
            livings.retain(|_, living| !Arc::ptr_eq(living, object));
        }
        let id = {
            let mut g = object.lock();
            if g.destructed {
                return Ok(());
            }
            g.destructed = true;
            g.interactive = None;
            g.pending_input = None;
            if let Some(env) = g.environment.take().and_then(|w| w.upgrade()) {
                let oid = g.id;
                env.lock()
                    .inventory
                    .retain(|item| !Arc::ptr_eq(item, object) && item.lock().id != oid);
            }
            let inventory = std::mem::take(&mut g.inventory);
            for child in inventory {
                child.lock().environment = None;
            }
            let name = g.name.clone();
            let id = g.id;
            let mut blueprints = self.blueprints.write();
            if blueprints.get(&name).copied() == Some(id) {
                blueprints.remove(&name);
            }
            id
        };
        self.objects.write().shift_remove(&id);
        Ok(())
    }

    pub fn move_object(&self, object: &ORef, destination: &ORef) -> Result<()> {
        {
            let mut g = object.lock();
            let oid = g.id;
            if let Some(env) = g.environment.take().and_then(|w| w.upgrade()) {
                // Do not re-lock `object` while its mutex is already held.
                env.lock()
                    .inventory
                    .retain(|item| !Arc::ptr_eq(item, object) && item.lock().id != oid);
            }
            g.environment = Some(Arc::downgrade(destination));
        }
        destination.lock().inventory.push(object.clone());
        let _ = self.apply(destination.clone(), "init", Vec::new(), None, None);
        let _ = self.apply(object.clone(), "init", Vec::new(), None, None);
        Ok(())
    }

    pub fn boot_master(&self) -> Result<ORef> {
        if let Some(path) = self.config.simul_efun.clone() {
            match self.load_object(&path) {
                Ok(object) => {
                    {
                        let mut guard = object.lock();
                        guard.uid = "Root".to_owned();
                        guard.euid = "Root".to_owned();
                    }
                    self.set_simul_efun(object);
                    tracing::info!(%path, "simul_efun object loaded");
                }
                Err(error) => {
                    tracing::warn!(
                        %path,
                        error = %format!("{error:#}"),
                        "failed to load simul_efun; continuing without it"
                    );
                }
            }
        }
        let master = self.load_object(&self.config.master.clone())?;
        {
            let mut guard = master.lock();
            guard.uid = "Root".to_owned();
            guard.euid = "Root".to_owned();
        }
        self.set_master(master.clone());
        Ok(master)
    }

    pub fn heartbeat(&self) {
        let objects = self.all_objects();
        for object in objects {
            let enabled = {
                let guard = object.lock();
                guard.heart_beat != 0 && guard.program.has_function("heart_beat")
            };
            if enabled {
                if let Err(error) = self.apply(object, "heart_beat", Vec::new(), None, None) {
                    tracing::warn!(error = %format!("{error:#}"), "heart_beat failed");
                }
            }
        }
    }

    pub fn process_call_outs(&self) {
        for entry in self.call_outs.due_now() {
            if entry.object.lock().destructed {
                continue;
            }
            let result = match &entry.fun {
                LpcValue::String(name) => self.apply(
                    entry.object.clone(),
                    name,
                    entry.args.clone(),
                    None,
                    None,
                ),
                LpcValue::Function(function) => {
                    let mut interpreter = Interpreter::new(
                        self,
                        entry.object.clone(),
                        None,
                        None,
                        self.config.max_cost,
                    );
                    interpreter.call_lpc_function(function, entry.args.clone())
                }
                other => Err(anyhow::anyhow!(
                    "call_out target must be string or function, got {}",
                    other.type_name()
                )),
            };
            if let Err(error) = result {
                tracing::warn!(error = %format!("{error:#}"), "call_out failed");
            }
        }
    }

    pub fn process_resets(&self) {
        for object in self.all_objects() {
            let should = {
                let mut guard = object.lock();
                if guard.destructed || !guard.program.has_function("reset") {
                    false
                } else {
                    guard.reset_count = guard.reset_count.wrapping_add(1);
                    guard.reset_count % 15 == 0
                }
            };
            if should {
                let _ = self.apply(object, "reset", Vec::new(), None, None);
            }
        }
    }

    /// Handle a line of player input: `input_to`, then `process_input`, then actions.
    pub fn handle_player_input(&self, player: ORef, line: String) -> Result<value::LpcValue> {
        self.handle_player_input_inner(player, line, true)
    }

    /// Like `handle_player_input` but skips `process_input` (used by the `command` efun).
    pub fn handle_player_command(&self, player: ORef, line: String) -> Result<value::LpcValue> {
        self.handle_player_input_inner(player, line, false)
    }

    fn handle_player_input_inner(
        &self,
        player: ORef,
        line: String,
        run_process_input: bool,
    ) -> Result<value::LpcValue> {
        // `command()` must not steal `input_to` callbacks (MudOS); only typed input does.
        if run_process_input {
            let pending = player.lock().pending_input.take();
            if let Some(pending) = pending {
                if pending.no_echo {
                    let _ = player.lock().set_echo(true);
                }
                let mut args = vec![value::LpcValue::String(line)];
                args.extend(pending.extra);
                return self.apply_input_callback(player, pending.fun, args);
            }
        }

        let mut command_line = line;
        if run_process_input {
            let has_process = {
                let program = player.lock().program.clone();
                Interpreter::find_function(&program, "process_input").is_some()
            };
            if has_process {
                let processed = self.apply(
                    player.clone(),
                    "process_input",
                    vec![value::LpcValue::String(command_line.clone())],
                    Some(player.clone()),
                    None,
                )?;
                match processed {
                    value::LpcValue::String(text) => command_line = text,
                    value::LpcValue::Null | value::LpcValue::Int(0) => {
                        return Ok(value::LpcValue::Int(1));
                    }
                    other if !other.is_truthy() => {
                        return Ok(value::LpcValue::Int(1));
                    }
                    other => command_line = other.to_string(),
                }
            }
        }

        let trimmed = command_line.trim().to_owned();
        if trimmed.is_empty() {
            return Ok(value::LpcValue::Int(1));
        }

        {
            let mut guard = player.lock();
            guard.last_verb = None;
            guard.notify_fail = None;
        }

        if self.try_command(&player, &trimmed)? {
            return Ok(value::LpcValue::Int(1));
        }

        if let Some(fail) = player.lock().notify_fail.take() {
            player.lock().write(fail);
        } else {
            player.lock().write("What?\n".to_owned());
        }
        Ok(value::LpcValue::Int(0))
    }

    fn try_command(&self, player: &ORef, line: &str) -> Result<bool> {
        let (verb, arg) = split_verb(line);
        player.lock().last_verb = Some(verb.clone());

        let mut targets = Vec::new();
        targets.push(player.clone());
        if let Some(env) = player.lock().environment() {
            targets.push(env.clone());
            let inventory = env.lock().inventory.clone();
            for item in inventory {
                if !Arc::ptr_eq(&item, player) {
                    targets.push(item);
                }
            }
        }
        let player_inv = player.lock().inventory.clone();
        targets.extend(player_inv);

        // Exact verb matches first, then catch-all (`add_action(fun, "", 1)`).
        // Catch-all receives the same trailing argument as a normal action (0 if none),
        // not the full input line — matching MudOS add_action semantics.
        if self.dispatch_actions(&targets, player, &verb, &arg, false)? {
            return Ok(true);
        }
        self.dispatch_actions(&targets, player, &verb, &arg, true)
    }

    fn dispatch_actions(
        &self,
        targets: &[ORef],
        player: &ORef,
        verb: &str,
        arg: &str,
        catch_all_pass: bool,
    ) -> Result<bool> {
        for target in targets {
            if target.lock().destructed {
                continue;
            }
            let actions = target.lock().actions.clone();
            for action in actions {
                let matches = if catch_all_pass {
                    if !action.catch_all {
                        false
                    } else if action.verb.is_empty() {
                        true
                    } else {
                        action.verb == verb
                            || arg == action.verb
                            || arg.starts_with(&format!("{} ", action.verb))
                    }
                } else {
                    !action.catch_all && action.verb == verb
                };
                if !matches {
                    continue;
                }
                // MudOS: bare verb → argument is 0 (falsy), not "".
                let args = if arg.is_empty() {
                    Vec::new()
                } else {
                    vec![value::LpcValue::String(arg.to_owned())]
                };
                let result = match &action.fun {
                    value::LpcValue::String(name) => self.apply(
                        target.clone(),
                        name,
                        args,
                        Some(player.clone()),
                        Some(player.clone()),
                    )?,
                    value::LpcValue::Function(function) => {
                        let mut interpreter = Interpreter::new(
                            self,
                            target.clone(),
                            Some(player.clone()),
                            Some(player.clone()),
                            self.config.max_cost,
                        );
                        interpreter.call_lpc_function(function, args)?
                    }
                    _ => continue,
                };
                if result.is_truthy() {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    fn apply_input_callback(
        &self,
        player: ORef,
        fun: value::LpcValue,
        arguments: Vec<value::LpcValue>,
    ) -> Result<value::LpcValue> {
        match fun {
            value::LpcValue::String(name) => {
                self.apply(player.clone(), &name, arguments, Some(player), None)
            }
            value::LpcValue::Function(function) => {
                let mut interpreter =
                    Interpreter::new(self, player.clone(), Some(player), None, self.config.max_cost);
                interpreter.call_lpc_function(&function, arguments)
            }
            other => anyhow::bail!(
                "input_to callback must be a string or function, got {}",
                other.type_name()
            ),
        }
    }

    pub fn request_shutdown(&self) {
        self.shutdown_requested.store(true, Ordering::Release);
    }

    pub fn is_shutting_down(&self) -> bool {
        self.shutdown_requested.load(Ordering::Acquire)
    }

    pub(crate) fn allocate_object_id(&self) -> Oid {
        self.next_object_id.fetch_add(1, Ordering::Relaxed)
    }

    pub(crate) fn allocate_clone_id(&self) -> u64 {
        self.next_clone_id.fetch_add(1, Ordering::Relaxed)
    }
}

fn split_verb(line: &str) -> (String, String) {
    let line = line.trim();
    if let Some((verb, rest)) = line.split_once(char::is_whitespace) {
        (verb.to_owned(), rest.trim_start().to_owned())
    } else {
        (line.to_owned(), String::new())
    }
}
