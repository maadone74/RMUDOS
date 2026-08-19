pub mod apply;
pub mod call_out;
pub mod fs_cache;
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
use anyhow::{bail, Context, Result};
use call_out::CallOutQueue;
use fs_cache::FsCache;
use indexmap::IndexMap;
use object::{Object as ObjectStruct, ObjectId as Oid, ObjectRef as ORef};
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

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
    /// MudOS is single-threaded: only one LPC evaluation may run at a time.
    /// Heartbeats and player input must not interleave or object mutexes deadlock.
    eval_lock: Mutex<()>,
    pub(crate) started: Instant,
    /// Stat / `get_dir` cache (MudOS-style; invalidated by write/rm/mkdir/…).
    pub(crate) fs_cache: FsCache,
    /// Blueprint paths currently running `create()` (reentrant `load_object` must return these).
    pub(crate) creating: RwLock<HashMap<String, ORef>>,
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
            eval_lock: Mutex::new(()),
            started: Instant::now(),
            fs_cache: FsCache::new(),
            creating: RwLock::new(HashMap::new()),
        }
    }

    /// Hold while running any LPC apply (input, heart_beat, call_out, connect).
    /// Do not take this from inside `apply` — `command()` re-enters on the same thread.
    pub fn lock_eval(&self) -> parking_lot::MutexGuard<'_, ()> {
        self.eval_lock.lock()
    }

    pub fn object(&self, id: Oid) -> Option<ORef> {
        self.objects.read().get(&id).cloned()
    }

    pub fn find_object(&self, path: &str) -> Option<ORef> {
        self.find_object_for(path, None)
    }

    pub fn find_object_for(&self, path: &str, viewer: Option<ORef>) -> Option<ORef> {
        let path = normalize_object_path(path);
        let object = if let Some((base, clone_suffix)) = path.split_once('#') {
            let clone_id = clone_suffix.parse::<u64>().ok()?;
            self.all_objects()
                .into_iter()
                .find(|object| {
                    let guard = object.lock();
                    guard.name == base && guard.clone_number == Some(clone_id)
                })?
        } else {
            let id = self.blueprints.read().get(&path).copied()?;
            self.object(id).filter(|object| !object.lock().destructed)?
        };
        self.visible_object(object, viewer)
    }

    fn visible_object(&self, object: ORef, viewer: Option<ORef>) -> Option<ORef> {
        if !object.lock().hidden {
            return Some(object);
        }
        viewer.filter(|viewer| viewer.lock().can_hide).map(|_| object)
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
        let objects: Vec<ORef> = self.objects.read().values().cloned().collect();
        objects
            .into_iter()
            .filter(|object| !object.lock().destructed)
            .collect()
    }

    pub fn users(&self) -> Vec<ORef> {
        let objects: Vec<ORef> = self.objects.read().values().cloned().collect();
        objects
            .into_iter()
            .filter(|object| {
                let object = object.lock();
                !object.destructed && object.interactive.is_some()
            })
            .collect()
    }

    pub fn apply_with_origin(
        &self,
        object: ORef,
        function: &str,
        arguments: Vec<value::LpcValue>,
        this_player: Option<ORef>,
        previous_object: Option<ORef>,
        origin: &'static str,
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
            interpreter.origin = origin;
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

    pub fn apply(
        &self,
        object: ORef,
        function: &str,
        arguments: Vec<value::LpcValue>,
        this_player: Option<ORef>,
        previous_object: Option<ORef>,
    ) -> Result<value::LpcValue> {
        self.apply_with_origin(
            object,
            function,
            arguments,
            this_player,
            previous_object,
            "call_other",
        )
    }

    pub fn load_object(&self, path: &str) -> Result<ORef> {
        let path = normalize_object_path(path);
        if let Some(existing) = self.find_object(&path) {
            return Ok(existing);
        }
        if let Some(partial) = self.creating.read().get(&path) {
            return Ok(partial.clone());
        }
        tracing::info!(%path, "load_object start");
        if !compiler::source_exists(&self.config.mudlib, &path) {
            return self.try_virtual_compile(&path);
        }
        let compile_started = Instant::now();
        let program = compiler::compile_file_in(&self.config.mudlib, &path)
            .with_context(|| format!("compile {path}"))?;
        tracing::info!(
            %path,
            elapsed_ms = compile_started.elapsed().as_millis(),
            "load_object compile ok"
        );
        let id = self.allocate_object_id();
        let object = Arc::new(Mutex::new(ObjectStruct::new(id, path.clone(), program)));
        self.creating.write().insert(path.clone(), object.clone());
        self.objects.write().insert(id, object.clone());
        self.blueprints.write().insert(path.clone(), id);
        tracing::info!(%path, "load_object create()");
        let create_result = self.apply(object.clone(), "create", Vec::new(), None, None);
        self.creating.write().remove(&path);
        if let Err(err) = create_result {
            self.objects.write().shift_remove(&id);
            self.blueprints.write().remove(&path);
            return Err(err).with_context(|| format!("create() in {path}"));
        }
        tracing::info!(%path, "load_object done");
        Ok(object)
    }

    pub fn clone_object(&self, path: &str) -> Result<ORef> {
        self.clone_object_from(path, None)
    }

    /// MudOS: `clone_object` of a path with no `.c` calls `master->compile_object`.
    /// The returned object is already the instance (virtual server clones itself).
    fn try_virtual_compile(&self, path: &str) -> Result<ORef> {
        let Some(master) = self.master() else {
            bail!("no master object for virtual compile of {path}");
        };
        tracing::info!(%path, "trying master compile_object");
        match self.apply(
            master,
            "compile_object",
            vec![value::LpcValue::String(path.to_owned())],
            None,
            None,
        ) {
            Ok(value::LpcValue::Object(object)) => {
                let id = {
                    let mut guard = object.lock();
                    guard.name = path.to_owned();
                    guard.id
                };
                self.blueprints.write().insert(path.to_owned(), id);
                tracing::info!(%path, "virtual compile_object ok");
                Ok(object)
            }
            Ok(value::LpcValue::String(resolved)) => self.load_object(&resolved),
            other => {
                tracing::warn!(
                    %path,
                    result = %format!("{other:?}"),
                    "compile_object did not return an object"
                );
                bail!("compile_object did not return an object for {path}: {other:?}")
            }
        }
    }

    /// MudOS: `previous_object()` during clone `create()` is the caller.
    pub fn clone_object_from(&self, path: &str, previous: Option<ORef>) -> Result<ORef> {
        let path = normalize_object_path(path);
        if !compiler::source_exists(&self.config.mudlib, &path) {
            return self.try_virtual_compile(&path);
        }
        let program = compiler::compile_file_in(&self.config.mudlib, &path)?;
        let id = self.allocate_object_id();
        let clone_number = self.allocate_clone_id();
        let mut object = ObjectStruct::new(id, path.clone(), program);
        object.clone_number = Some(clone_number);
        let object = Arc::new(Mutex::new(object));
        self.objects.write().insert(id, object.clone());
        self.apply(object.clone(), "create", Vec::new(), None, previous)
            .with_context(|| format!("create() in clone of {path}"))?;
        Ok(object)
    }

    pub fn destruct_object(&self, object: &ORef) -> Result<()> {
        self.call_outs.remove_object(object);
        {
            let mut livings = self.livings.write();
            livings.retain(|_, living| !Arc::ptr_eq(living, object));
        }
        let (id, old_shadow, old_shadowed) = {
            let mut g = object.lock();
            if g.destructed {
                return Ok(());
            }
            g.destructed = true;
            g.interactive = None;
            g.pending_input = None;
            let old_shadow = g.shadow.take();
            let old_shadowed = g.shadowed.take();
            if let Some(env) = g.environment.take().and_then(|w| w.upgrade()) {
                let oid = g.id;
                env.lock().inventory.retain(|item| {
                    !Arc::ptr_eq(item, object)
                        && item.try_lock().map(|g| g.id != oid).unwrap_or(true)
                });
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
            (id, old_shadow, old_shadowed)
        };
        if let Some(shadow) = old_shadow {
            shadow.lock().shadowed = None;
        }
        if let Some(target) = old_shadowed.and_then(|weak| weak.upgrade()) {
            let mut target = target.lock();
            if target
                .shadow
                .as_ref()
                .is_some_and(|shadow| Arc::ptr_eq(shadow, object))
            {
                target.shadow = None;
            }
        }
        self.objects.write().shift_remove(&id);
        Ok(())
    }

    pub fn move_object(&self, object: &ORef, destination: &ORef) -> Result<()> {
        let old_env = object.lock().environment();
        {
            let mut g = object.lock();
            let oid = g.id;
            if let Some(env) = g.environment.take().and_then(|w| w.upgrade()) {
                // Do not re-lock `object` (or other inventory) while `object` is held.
                env.lock().inventory.retain(|item| {
                    !Arc::ptr_eq(item, object)
                        && item.try_lock().map(|g| g.id != oid).unwrap_or(true)
                });
            }
            g.environment = Some(Arc::downgrade(destination));
            g.move_count = g.move_count.saturating_add(1);
        }
        destination.lock().inventory.push(object.clone());
        if Self::object_is_living(object) {
            // Living enters dest: this_player is the mover.
            self.prune_stale_actions(object);
            self.call_init(destination, object);
            let inventory = destination.lock().inventory.clone();
            for item in inventory {
                if Arc::ptr_eq(&item, object) || item.lock().destructed {
                    continue;
                }
                self.call_init(&item, object);
            }
            self.call_init(object, object);
        } else {
            // Item moved: init the item for each living that can now see it
            // (inventory dest, or livings in a room). Do not treat the item as
            // this_player — that stored add_action on the object, not the player.
            if let Some(old) = old_env {
                for living in Self::livings_here(&old) {
                    self.prune_stale_actions(&living);
                }
            }
            for living in Self::livings_here(destination) {
                self.call_init(object, &living);
            }
        }
        Ok(())
    }

    fn object_is_living(object: &ORef) -> bool {
        let guard = object.lock();
        !guard.destructed && (guard.commands_enabled || guard.living_name.is_some())
    }

    fn livings_here(place: &ORef) -> Vec<ORef> {
        let mut out = Vec::new();
        if Self::object_is_living(place) {
            out.push(place.clone());
        }
        for item in place.lock().inventory.clone() {
            if Self::object_is_living(&item) {
                out.push(item);
            }
        }
        out
    }

    fn call_init(&self, object: &ORef, this_player: &ORef) {
        if let Err(error) = self.apply(
            object.clone(),
            "init",
            Vec::new(),
            Some(this_player.clone()),
            Some(this_player.clone()),
        ) {
            tracing::warn!(
                error = %format!("{error:#}"),
                path = %object.lock().name,
                "init() failed"
            );
        }
    }

    /// Remove `add_action` sentences whose owner is no longer near the living.
    fn prune_stale_actions(&self, living: &ORef) {
        let mut nearby: Vec<ORef> = vec![living.clone()];
        if let Some(env) = living.lock().environment() {
            nearby.push(env.clone());
            nearby.extend(env.lock().inventory.clone());
        }
        nearby.extend(living.lock().inventory.clone());
        living.lock().actions.retain(|action| {
            nearby
                .iter()
                .any(|object| Arc::ptr_eq(object, &action.owner))
        });
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
                let name = object.lock().name.clone();
                tracing::debug!(%name, "heart_beat start");
                if let Err(error) =
                    self.apply_with_origin(object, "heart_beat", Vec::new(), None, None, "driver")
                {
                    tracing::warn!(%name, error = %format!("{error:#}"), "heart_beat failed");
                }
                tracing::debug!(%name, "heart_beat end");
            }
        }
    }

    pub fn process_call_outs(&self) {
        for entry in self.call_outs.due_now() {
            if entry.object.lock().destructed {
                continue;
            }
            let result = match &entry.fun {
                LpcValue::String(name) => self.apply_with_origin(
                    entry.object.clone(),
                    name,
                    entry.args.clone(),
                    None,
                    None,
                    "call_out",
                ),
                LpcValue::Function(function) => {
                    let mut interpreter = Interpreter::new(
                        self,
                        entry.object.clone(),
                        None,
                        None,
                        self.config.max_cost,
                    );
                    interpreter.origin = "call_out";
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
        let result = self.handle_player_input_core(player.clone(), line, run_process_input);
        if run_process_input {
            self.maybe_write_prompt(&player);
        }
        result
    }

    /// MudOS: after a typed line, apply `write_prompt` unless `input_to` is pending
    /// (pager, password, editor). `command()` does not reprint the prompt.
    fn maybe_write_prompt(&self, player: &ORef) {
        if player.lock().destructed || player.lock().pending_input.is_some() {
            return;
        }
        let has_prompt = {
            let program = player.lock().program.clone();
            Interpreter::find_function(&program, "write_prompt").is_some()
        };
        if !has_prompt {
            return;
        }
        if let Err(error) = self.apply(
            player.clone(),
            "write_prompt",
            Vec::new(),
            Some(player.clone()),
            None,
        ) {
            tracing::debug!(error = %format!("{error:#}"), "write_prompt failed");
        }
    }

    fn handle_player_input_core(
        &self,
        player: ORef,
        line: String,
        run_process_input: bool,
    ) -> Result<value::LpcValue> {
        // `command()` must not steal `input_to` callbacks (MudOS); only typed input does.
        if run_process_input {
            let pending = player.lock().pending_input.take();
            if let Some(pending) = pending {
                // Keep a copy so a failed callback does not drop the prompt and fall
                // through to cmd_hook (which can hang on FS-backed command rehash).
                let restore = pending.clone();
                if pending.no_echo {
                    let _ = player.lock().set_echo(true);
                }
                // Trim like normal commands so telnet CR/space-only lines work in pagers.
                let line = line.trim_end_matches(['\r', '\n']).to_string();
                let mut args = vec![value::LpcValue::String(line)];
                args.extend(pending.extra);
                let result =
                    self.apply_input_callback(player.clone(), pending.owner, pending.fun, args);
                if let Err(error) = &result {
                    player.lock().pending_input = Some(restore);
                    player.lock().write(format!(
                        "Error: {error:#}\n(Still waiting for input — try again or type q.)\n"
                    ));
                    tracing::warn!(error = %format!("{error:#}"), "input_to callback failed");
                    return Ok(value::LpcValue::Int(0));
                }
                return result;
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

        tracing::info!(command = %trimmed, "player command start");

        {
            let mut guard = player.lock();
            guard.last_verb = None;
            guard.notify_fail = None;
        }

        let handled = self.try_command(&player, &trimmed)?;
        tracing::info!(command = %trimmed, handled, "player command done");

        if handled {
            return Ok(value::LpcValue::Int(1));
        }

        let mut guard = player.lock();
        if let Some(fail) = guard.notify_fail.take() {
            guard.write(fail);
        } else {
            guard.write("What?\n".to_owned());
        }
        Ok(value::LpcValue::Int(0))
    }

    fn try_command(&self, player: &ORef, line: &str) -> Result<bool> {
        let (verb, arg) = split_verb(line);
        player.lock().last_verb = Some(verb.clone());

        // MudOS: all sentences live on the command giver (the living).
        // Exact verb matches first, then catch-all (`add_action(fun, "", 1)`).
        // Catch-all receives the same trailing argument as a normal action (0 if none),
        // not the full input line — matching MudOS add_action semantics.
        if self.dispatch_actions(player, &verb, &arg, false)? {
            return Ok(true);
        }
        self.dispatch_actions(player, &verb, &arg, true)
    }

    fn dispatch_actions(
        &self,
        player: &ORef,
        verb: &str,
        arg: &str,
        catch_all_pass: bool,
    ) -> Result<bool> {
        let actions = player.lock().actions.clone();
        // MudOS tries the most recently added action first (LIFO). Rooms
        // register use_stupid_exit for all compass verbs, then use_exit for
        // real exits — FIFO would always hit "You cannot go that way."
        for action in actions.into_iter().rev() {
            if action.owner.lock().destructed {
                continue;
            }
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
                    action.owner.clone(),
                    name,
                    args,
                    Some(player.clone()),
                    Some(player.clone()),
                )?,
                value::LpcValue::Function(function) => {
                    let mut interpreter = Interpreter::new(
                        self,
                        action.owner.clone(),
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
        Ok(false)
    }

    fn apply_input_callback(
        &self,
        player: ORef,
        owner: ORef,
        fun: value::LpcValue,
        arguments: Vec<value::LpcValue>,
    ) -> Result<value::LpcValue> {
        match fun {
            value::LpcValue::String(name) => {
                // Callback runs on the object that called input_to; this_player is the interactive.
                self.apply(owner, &name, arguments, Some(player), None)
            }
            value::LpcValue::Function(function) => {
                let mut interpreter =
                    Interpreter::new(self, owner, Some(player), None, self.config.max_cost);
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
