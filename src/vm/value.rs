use super::object::ObjectRef;
use indexmap::IndexMap;
use std::fmt;

#[derive(Clone, Debug)]
pub enum LpcValue {
    Null,
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<LpcValue>),
    Mapping(IndexMap<String, LpcValue>),
    Object(ObjectRef),
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
                    left.lock().id == right.lock().id
                }
            }
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
            Self::Array(_) | Self::Mapping(_) | Self::Object(_) => {
                formatter.write_str(&self.lpc_repr())
            }
        }
    }
}
