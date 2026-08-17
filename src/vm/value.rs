use super::object::ObjectRef;
use super::program::FunctionInfo;
use indexmap::IndexMap;
use parking_lot::Mutex;
use std::fmt;
use std::sync::Arc;

/// First-class MudOS-style function value (`(: … :)`).
#[derive(Clone, Debug)]
pub struct LpcFunction {
    pub owner: ObjectRef,
    pub kind: FunctionKind,
}

#[derive(Clone, Debug)]
pub enum FunctionKind {
    /// `(: name :)` / `(: name, bound... :)` — object apply or efun.
    Named { name: String, bound: Vec<LpcValue> },
    /// `(: expression :)` compiled anonymous body.
    Expression { function: Arc<FunctionInfo> },
}

/// Compile-time class (struct) definition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClassDef {
    pub name: String,
    pub fields: Vec<String>,
}

/// Runtime class instance (shared reference semantics).
#[derive(Debug)]
pub struct ClassInstance {
    pub def: Arc<ClassDef>,
    pub fields: Mutex<Vec<LpcValue>>,
}

impl ClassInstance {
    pub fn new(def: Arc<ClassDef>) -> Arc<Self> {
        let fields = vec![LpcValue::Null; def.fields.len()];
        Arc::new(Self {
            def,
            fields: Mutex::new(fields),
        })
    }
}

#[derive(Clone, Debug)]
pub enum LpcValue {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<LpcValue>),
    Mapping(IndexMap<String, LpcValue>),
    Object(ObjectRef),
    Function(Arc<LpcFunction>),
    Class(Arc<ClassInstance>),
}

impl LpcValue {
    pub fn is_truthy(&self) -> bool {
        match self {
            Self::Null => false,
            Self::Int(value) => *value != 0,
            Self::Float(value) => *value != 0.0,
            Self::String(value) => !value.is_empty(),
            Self::Array(value) => !value.is_empty(),
            Self::Mapping(value) => !value.is_empty(),
            Self::Object(object) => !object.lock().destructed,
            Self::Function(_) => true,
            Self::Class(_) => true,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(value) => Some(*value),
            Self::Float(value) => Some(*value as i64),
            Self::String(value) => value.trim().parse().ok(),
            Self::Null => Some(0),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn into_string(self) -> Option<String> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Null => "null",
            Self::Int(_) => "int",
            Self::Float(_) => "float",
            Self::String(_) => "string",
            Self::Array(_) => "array",
            Self::Mapping(_) => "mapping",
            Self::Object(_) => "object",
            Self::Function(_) => "function",
            Self::Class(_) => "class",
        }
    }

    pub fn lpc_repr(&self) -> String {
        match self {
            Self::Null => "0".to_owned(),
            Self::Int(value) => value.to_string(),
            Self::Float(value) => value.to_string(),
            Self::String(value) => format!("{value:?}"),
            Self::Array(values) => format!(
                "({{ {} }})",
                values
                    .iter()
                    .map(Self::lpc_repr)
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Mapping(values) => format!(
                "([ {} ])",
                values
                    .iter()
                    .map(|(key, value)| format!("{key:?}: {}", value.lpc_repr()))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Object(object) => format!("<{}>", object.lock().name),
            Self::Function(function) => match &function.kind {
                FunctionKind::Named { name, bound } => {
                    if bound.is_empty() {
                        format!("(: {name} :)")
                    } else {
                        format!("(: {name}, ... :)")
                    }
                }
                FunctionKind::Expression { .. } => "(: <expr> :)".to_owned(),
            },
            Self::Class(instance) => {
                let fields = instance.fields.lock();
                format!(
                    "(#\"{}\",{})",
                    instance.def.name,
                    fields
                        .iter()
                        .map(Self::lpc_repr)
                        .collect::<Vec<_>>()
                        .join(",")
                )
            }
        }
    }
}

impl Default for LpcValue {
    fn default() -> Self {
        Self::Null
    }
}

impl PartialEq for LpcValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Int(left), Self::Int(right)) => left == right,
            (Self::Float(left), Self::Float(right)) => left == right,
            (Self::Int(left), Self::Float(right)) => *left as f64 == *right,
            (Self::Float(left), Self::Int(right)) => *left == *right as f64,
            (Self::String(left), Self::String(right)) => left == right,
            (Self::Array(left), Self::Array(right)) => left == right,
            (Self::Mapping(left), Self::Mapping(right)) => left == right,
            (Self::Object(left), Self::Object(right)) => {
                if std::sync::Arc::ptr_eq(left, right) {
                    true
                } else {
                    // Never nest object mutexes: same-thread deadlock if both
                    // Arcs alias one object, or lock-order inversion across threads.
                    match (left.try_lock(), right.try_lock()) {
                        (Some(a), Some(b)) => a.id == b.id,
                        _ => false,
                    }
                }
            }
            (Self::Function(left), Self::Function(right)) => Arc::ptr_eq(left, right),
            (Self::Class(left), Self::Class(right)) => Arc::ptr_eq(left, right),
            _ => false,
        }
    }
}

impl From<&str> for LpcValue {
    fn from(value: &str) -> Self {
        Self::String(value.to_owned())
    }
}

impl From<String> for LpcValue {
    fn from(value: String) -> Self {
        Self::String(value)
    }
}

impl From<i64> for LpcValue {
    fn from(value: i64) -> Self {
        Self::Int(value)
    }
}

impl fmt::Display for LpcValue {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Null => write!(formatter, "0"),
            Self::Int(value) => write!(formatter, "{value}"),
            Self::Float(value) => write!(formatter, "{value}"),
            Self::String(value) => formatter.write_str(value),
            Self::Array(_)
            | Self::Mapping(_)
            | Self::Object(_)
            | Self::Function(_)
            | Self::Class(_) => formatter.write_str(&self.lpc_repr()),
        }
    }
}
