//! Deferred `call_out` scheduling.

use crate::vm::object::ObjectRef;
use crate::vm::value::LpcValue;
use parking_lot::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct CallOut {
    pub id: u64,
    pub object: ObjectRef,
    pub fun: LpcValue,
    pub args: Vec<LpcValue>,
    pub due: Instant,
}

#[derive(Default)]
pub struct CallOutQueue {
    next_id: AtomicU64,
    entries: Mutex<Vec<CallOut>>,
}

impl CallOutQueue {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn schedule(
        &self,
        object: ObjectRef,
        fun: LpcValue,
        delay_secs: f64,
        args: Vec<LpcValue>,
    ) -> u64 {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        let due = Instant::now() + Duration::from_secs_f64(delay_secs.max(0.0));
        self.entries.lock().push(CallOut {
            id,
            object,
            fun,
            args,
            due,
        });
        id
    }

    pub fn remove(&self, id: i64) -> i64 {
        if id <= 0 {
            return 0;
        }
        let mut entries = self.entries.lock();
        let before = entries.len();
        entries.retain(|entry| entry.id != id as u64);
        (before - entries.len()) as i64
    }

    pub fn remove_object(&self, object: &ObjectRef) {
        let mut entries = self.entries.lock();
        entries.retain(|entry| !std::sync::Arc::ptr_eq(&entry.object, object));
    }

    pub fn find(&self, object: &ObjectRef, name: &str) -> i64 {
        let entries = self.entries.lock();
        for entry in entries.iter() {
            if !std::sync::Arc::ptr_eq(&entry.object, object) {
                continue;
            }
            match &entry.fun {
                LpcValue::String(fun) if fun == name => {
                    let remaining = entry
                        .due
                        .saturating_duration_since(Instant::now())
                        .as_secs_f64()
                        .ceil() as i64;
                    return remaining.max(0);
                }
                _ => {}
            }
        }
        -1
    }

    pub fn due_now(&self) -> Vec<CallOut> {
        let now = Instant::now();
        let mut entries = self.entries.lock();
        let mut due = Vec::new();
        entries.retain(|entry| {
            if entry.due <= now {
                due.push(entry.clone());
                false
            } else {
                true
            }
        });
        due
    }
}
