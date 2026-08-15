use super::program::Program;
use super::value::LpcValue;
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
    output: mpsc::UnboundedSender<String>,
}

impl Interactive {
    pub fn new(
        peer: SocketAddr,
        name: impl Into<String>,
        output: mpsc::UnboundedSender<String>,
    ) -> Self {
        Self {
            peer,
            name: name.into(),
            output,
        }
    }

    pub fn write(&self, message: impl Into<String>) -> bool {
        self.output.send(message.into()).is_ok()
    }
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
}

impl Object {
    pub fn new(id: ObjectId, name: String, program: Arc<Program>) -> Self {
        let global_count = program.globals.len();
        Self {
            id,
            name,
            program,
            globals: vec![LpcValue::Null; global_count],
            environment: None,
            inventory: Vec::new(),
            interactive: None,
            destructed: false,
            clone_number: None,
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
        self.interactive
            .as_ref()
            .is_some_and(|interactive| interactive.write(message))
    }
}
