pub mod apply;
pub mod interpret;
pub mod object;
pub mod program;
pub mod value;

pub use interpret::Interpreter;
pub use object::{Interactive, Object, ObjectId, ObjectRef};
pub use value::LpcValue;

use crate::compiler;
use crate::config::{normalize_object_path, DriverConfig};
use crate::efun::EfunTable;
use anyhow::{Context, Result};
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
    pub(crate) next_object_id: AtomicU64,
    pub(crate) next_clone_id: AtomicU64,
    pub shutdown_requested: AtomicBool,
}

impl MudWorld {
    pub fn new(config: DriverConfig) -> Self {
        Self {
            config,
            efuns: EfunTable::new(),
            objects: RwLock::new(IndexMap::new()),
            blueprints: RwLock::new(HashMap::new()),
            master: RwLock::new(None),
            next_object_id: AtomicU64::new(1),
            next_clone_id: AtomicU64::new(1),
            shutdown_requested: AtomicBool::new(false),
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
        let mut interpreter =
            Interpreter::new(self, object, this_player, previous_object, self.config.max_cost);
        // Missing applies are soft-noops for optional hooks.
        let program = interpreter.current_object.lock().program.clone();
        if Interpreter::find_function(&program, function).is_none() {
            return Ok(value::LpcValue::Null);
        }
        interpreter.apply(function, arguments)
    }

    pub fn load_object(&self, path: &str) -> Result<ORef> {
        let path = normalize_object_path(path);
        if let Some(existing) = self.find_object(&path) {
            return Ok(existing);
        }
        let program = compiler::compile_file_in(&self.config.mudlib, &path)?;
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
        let id = {
            let mut g = object.lock();
            if g.destructed {
                return Ok(());
            }
            g.destructed = true;
            g.interactive = None;
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
        let master = self.load_object(&self.config.master.clone())?;
        self.set_master(master.clone());
        Ok(master)
    }

    pub fn heartbeat(&self) {
        let objects = self.all_objects();
        for object in objects {
            let has_heartbeat = object.lock().program.has_function("heart_beat");
            if has_heartbeat {
                if let Err(error) = self.apply(object, "heart_beat", Vec::new(), None, None) {
                    tracing::warn!(%error, "heart_beat failed");
                }
            }
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
