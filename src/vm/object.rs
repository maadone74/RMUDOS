use super::program::Program;
use super::value::LpcValue;
use crate::net::TelnetOut;
use parking_lot::Mutex;
use std::net::SocketAddr;
use std::sync::{Arc, Weak};
use tokio::sync::mpsc;

pub type ObjectId = u64;
pub type ObjectRef = Arc<Mutex<Object>>;

#[derive(Debug)]
pub struct Interactive {
    pub peer: SocketAddr,
    pub name: String,
    output: mpsc::UnboundedSender<TelnetOut>,
}

impl Interactive {
    pub fn new(
        peer: SocketAddr,
        name: impl Into<String>,
        output: mpsc::UnboundedSender<TelnetOut>,
    ) -> Self {
        Self {
            peer,
            name: name.into(),
            output,
        }
    }

    pub fn write(&self, message: impl Into<String>) -> bool {
        self.output.send(TelnetOut::Text(message.into())).is_ok()
    }

    pub fn set_echo(&self, enable: bool) -> bool {
        self.output.send(TelnetOut::Echo(enable)).is_ok()
    }
}

/// Next line of input diverted by `input_to`.
#[derive(Clone, Debug)]
pub struct PendingInput {
    /// Object that called `input_to` (callback is applied here for string names).
    pub owner: ObjectRef,
    pub fun: LpcValue,
    pub extra: Vec<LpcValue>,
    pub no_echo: bool,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub verb: String,
    pub fun: LpcValue,
    /// MudOS flag: empty verb with flag treats this as a catch-all command.
    pub catch_all: bool,
    /// Object that called `add_action` — function is applied here (MudOS `sentence->ob`).
    pub owner: ObjectRef,
}

#[derive(Debug)]
pub struct Object {
    pub id: ObjectId,
    pub name: String,
    pub program: Arc<Program>,
    pub globals: Vec<LpcValue>,
    pub environment: Option<Weak<Mutex<Object>>>,
    pub inventory: Vec<ObjectRef>,
    pub interactive: Option<Arc<Interactive>>,
    pub destructed: bool,
    pub clone_number: Option<u64>,
    /// Non-zero when `set_heart_beat` has enabled heartbeats (MudOS semantics).
    pub heart_beat: i32,
    pub pending_input: Option<PendingInput>,
    pub actions: Vec<Action>,
    pub notify_fail: Option<String>,
    pub last_verb: Option<String>,
    pub commands_enabled: bool,
    pub uid: String,
    pub euid: String,
    pub wizard: bool,
    pub living_name: Option<String>,
    pub shadow: Option<ObjectRef>,
    pub shadowed: Option<Weak<Mutex<Object>>>,
    pub reset_count: u64,
    pub nosave_globals: Vec<bool>,
    /// MudOS `set_hide`: excluded from `find_object` unless viewer is hidable.
    pub hidden: bool,
    /// Set when `master()->valid_hide(ob)` succeeds; hidable viewers see hidden objects.
    pub can_hide: bool,
    /// Who is snooping this object (MudOS `query_snoop`).
    pub snooper: Option<ObjectRef>,
    /// Who this object is snooping (MudOS `query_snooping` / one-arg `snoop`).
    pub snoop_target: Option<ObjectRef>,
    /// MudOS `in_edit` — non-empty while `ed()` session is active.
    pub editing_file: Option<String>,
}

impl Object {
    pub fn new(id: ObjectId, name: String, program: Arc<Program>) -> Self {
        let global_count = program.globals.len();
        let nosave_globals = if program.nosave_globals.len() == global_count {
            program.nosave_globals.clone()
        } else {
            vec![false; global_count]
        };
        let uid = name.clone();
        Self {
            id,
            name: name.clone(),
            program,
            globals: vec![LpcValue::Null; global_count],
            environment: None,
            inventory: Vec::new(),
            interactive: None,
            destructed: false,
            clone_number: None,
            heart_beat: 0,
            pending_input: None,
            actions: Vec::new(),
            notify_fail: None,
            last_verb: None,
            commands_enabled: false,
            uid: uid.clone(),
            euid: uid,
            wizard: false,
            living_name: None,
            shadow: None,
            shadowed: None,
            reset_count: 0,
            nosave_globals,
            hidden: false,
            can_hide: false,
            snooper: None,
            snoop_target: None,
            editing_file: None,
        }
    }

    pub fn environment(&self) -> Option<ObjectRef> {
        self.environment.as_ref().and_then(Weak::upgrade)
    }

    pub fn file_name(&self) -> String {
        match self.clone_number {
            Some(number) => format!("{}#{number}", self.name),
            None => self.name.clone(),
        }
    }

    pub fn write(&self, message: impl Into<String>) -> bool {
        let message = message.into();
        let mut ok = false;
        if let Some(interactive) = self.interactive.as_ref() {
            ok = interactive.write(&message);
        }
        if let Some(snooper) = self.snooper.as_ref() {
            let label = self
                .living_name
                .as_deref()
                .unwrap_or(self.name.as_str());
            let _ = snooper.lock().write(format!("%{label} {message}"));
        }
        ok
    }

    pub fn set_echo(&self, enable: bool) -> bool {
        self.interactive
            .as_ref()
            .is_some_and(|interactive| interactive.set_echo(enable))
    }
}
