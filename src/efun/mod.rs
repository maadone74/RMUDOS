use crate::vm::interpret::Interpreter;
use crate::vm::object::ObjectRef;
use crate::vm::value::LpcValue;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub type EfunFn = for<'a> fn(&mut Interpreter<'a>, Vec<LpcValue>) -> Result<LpcValue>;

pub struct EfunTable {
    functions: IndexMap<&'static str, EfunFn>,
}

impl EfunTable {
    pub fn new() -> Self {
        let mut functions: IndexMap<&'static str, EfunFn> = IndexMap::new();
        functions.insert("write", write);
        functions.insert("say", say);
        functions.insert("tell_object", tell_object);
        functions.insert("tell_room", tell_room);
        functions.insert("message", message);
        functions.insert("capitalize", capitalize);
        functions.insert("lower_case", lower_case);
        functions.insert("strlen", strlen);
        functions.insert("explode", explode);
        functions.insert("implode", implode);
        functions.insert("member_array", member_array);
        functions.insert("sizeof", sizeof);
        functions.insert("keys", keys);
        functions.insert("values", values);
        functions.insert("clone_object", clone_object);
        functions.insert("load_object", load_object);
        functions.insert("find_object", find_object);
        functions.insert("destruct", destruct);
        functions.insert("move_object", move_object);
        functions.insert("environment", environment);
        functions.insert("all_inventory", all_inventory);
        functions.insert("file_name", file_name);
        functions.insert("this_player", this_player);
        functions.insert("previous_object", previous_object);
        functions.insert("users", users);
        functions.insert("call_other", call_other);
        functions.insert("sprintf", sprintf);
        functions.insert("atoi", atoi);
        functions.insert("to_string", to_string);
        functions.insert("typeof", type_of);
        functions.insert("time", time);
        functions.insert("debug_message", debug_message);
        functions.insert("shutdown", shutdown);
        Self { functions }
    }

    pub fn get(&self, name: &str) -> Option<EfunFn> {
        self.functions.get(name).copied()
    }

    pub fn names(&self) -> Vec<&'static str> {
        self.functions.keys().copied().collect()
    }
}

impl Default for EfunTable {
    fn default() -> Self {
        Self::new()
    }
}

fn write(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "write")?;
    let message = arguments
        .iter()
        .map(ToString::to_string)
        .collect::<String>();
    let recipient = interpreter
        .this_player
        .clone()
        .unwrap_or_else(|| interpreter.current_object.clone());
    recipient.lock().write(message);
    Ok(LpcValue::Int(1))
}

fn say(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "say")?;
    let message = arguments[0].to_string();
    let actor = interpreter
        .this_player
        .clone()
        .unwrap_or_else(|| interpreter.current_object.clone());
    let room = actor.lock().environment();
    if let Some(room) = room {
        deliver_room(&room, &message, &[actor]);
    }
    Ok(LpcValue::Int(1))
}

fn tell_object(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "tell_object")?;
    let target = object_argument(&arguments[0], "tell_object")?;
    target.lock().write(arguments[1].to_string());
    Ok(LpcValue::Int(1))
}

fn tell_room(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "tell_room")?;
    let room = resolve_object(interpreter, &arguments[0], false)?
        .context("tell_room target does not exist")?;
    let excludes = arguments
        .get(2)
        .map(objects_from_value)
        .unwrap_or_default();
    deliver_room(&room, &arguments[1].to_string(), &excludes);
    Ok(LpcValue::Int(1))
}

fn message(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 3, "message")?;
    let text = arguments[1].to_string();
    let excludes = arguments
        .get(3)
        .map(objects_from_value)
        .unwrap_or_default();
    deliver_target(interpreter, &arguments[2], &text, &excludes)?;
    Ok(LpcValue::Int(1))
}

fn capitalize(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let value = string_argument(&arguments, 0, "capitalize")?;
    let mut chars = value.chars();
    let result = match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    };
    Ok(LpcValue::String(result))
}

fn lower_case(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::String(
        string_argument(&arguments, 0, "lower_case")?.to_lowercase(),
    ))
}

fn strlen(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::Int(
        string_argument(&arguments, 0, "strlen")?.chars().count() as i64,
    ))
}

fn explode(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let value = string_argument(&arguments, 0, "explode")?;
    let separator = string_argument(&arguments, 1, "explode")?;
    let values = if separator.is_empty() {
        value
            .chars()
            .map(|ch| LpcValue::String(ch.to_string()))
            .collect()
    } else {
        value
            .split(separator)
            .filter(|part| !part.is_empty())
            .map(|part| LpcValue::String(part.to_owned()))
            .collect()
    };
    Ok(LpcValue::Array(values))
}

fn implode(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "implode")?;
    let LpcValue::Array(values) = &arguments[0] else {
        bail!("implode argument 1 must be an array");
    };
    let separator = arguments[1]
        .as_string()
        .context("implode argument 2 must be a string")?;
    Ok(LpcValue::String(
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(separator),
    ))
}

fn member_array(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "member_array")?;
    let start = arguments
        .get(2)
        .and_then(LpcValue::as_int)
        .unwrap_or(0)
        .max(0) as usize;
    let index = match &arguments[1] {
        LpcValue::Array(values) => values
            .iter()
            .enumerate()
            .skip(start)
            .find_map(|(index, value)| (value == &arguments[0]).then_some(index)),
        LpcValue::String(value) => {
            let needle = arguments[0].to_string();
            value
                .char_indices()
                .skip(start)
                .find_map(|(index, _)| value[index..].starts_with(&needle).then_some(index))
        }
        _ => bail!("member_array argument 2 must be an array or string"),
    };
    Ok(LpcValue::Int(index.map_or(-1, |index| index as i64)))
}

fn sizeof(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "sizeof")?;
    let size = match &arguments[0] {
        LpcValue::Null => 0,
        LpcValue::String(value) => value.chars().count(),
        LpcValue::Array(value) => value.len(),
        LpcValue::Mapping(value) => value.len(),
        _ => bail!("sizeof requires a string, array, or mapping"),
    };
    Ok(LpcValue::Int(size as i64))
}

fn keys(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "keys")?;
    let LpcValue::Mapping(mapping) = &arguments[0] else {
        bail!("keys requires a mapping");
    };
    Ok(LpcValue::Array(
        mapping.keys().cloned().map(LpcValue::String).collect(),
    ))
}

fn values(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "values")?;
    let LpcValue::Mapping(mapping) = &arguments[0] else {
        bail!("values requires a mapping");
    };
    Ok(LpcValue::Array(mapping.values().cloned().collect()))
}

fn clone_object(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let path = string_argument(&arguments, 0, "clone_object")?;
    Ok(LpcValue::Object(interpreter.world.clone_object(path)?))
}

fn load_object(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let path = string_argument(&arguments, 0, "load_object")?;
    Ok(LpcValue::Object(interpreter.world.load_object(path)?))
}

fn find_object(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let path = string_argument(&arguments, 0, "find_object")?;
    Ok(interpreter
        .world
        .find_object(path)
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null))
}

fn destruct(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "destruct")?;
    let target = object_argument(&arguments[0], "destruct")?;
    interpreter.world.destruct_object(&target)?;
    Ok(LpcValue::Int(1))
}

fn move_object(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    // MudOS allows move_object(dest) meaning move this_object.
    let (object, destination) = if arguments.len() >= 2 {
        let object = object_argument(&arguments[0], "move_object")?;
        let destination = resolve_object(interpreter, &arguments[1], true)?
            .context("move_object destination does not exist")?;
        (object, destination)
    } else {
        require(&arguments, 1, "move_object")?;
        let destination = resolve_object(interpreter, &arguments[0], true)?
            .context("move_object destination does not exist")?;
        (interpreter.current_object.clone(), destination)
    };
    interpreter.world.move_object(&object, &destination)?;
    Ok(LpcValue::Int(1))
}

fn environment(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        object_argument(value, "environment")?
    } else {
        interpreter.current_object.clone()
    };
    let env = object.lock().environment();
    Ok(env.map(LpcValue::Object).unwrap_or(LpcValue::Null))
}

fn all_inventory(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        object_argument(value, "all_inventory")?
    } else {
        interpreter.current_object.clone()
    };
    let inventory = object.lock().inventory.clone();
    Ok(LpcValue::Array(
        inventory.into_iter().map(LpcValue::Object).collect(),
    ))
}

fn file_name(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        object_argument(value, "file_name")?
    } else {
        interpreter.current_object.clone()
    };
    let name = object.lock().file_name();
    Ok(LpcValue::String(name))
}

fn this_player(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(interpreter
        .this_player
        .clone()
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null))
}

fn previous_object(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(interpreter
        .previous_object
        .clone()
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null))
}

fn users(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::Array(
        interpreter
            .world
            .users()
            .into_iter()
            .map(LpcValue::Object)
            .collect(),
    ))
}

fn call_other(
    interpreter: &mut Interpreter<'_>,
    mut arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "call_other")?;
    let target_value = arguments.remove(0);
    let function = arguments
        .remove(0)
        .into_string()
        .context("call_other function name must be a string")?;
    let target = resolve_object(interpreter, &target_value, true)?
        .context("call_other target does not exist")?;
    interpreter.call_function(target, &function, arguments)
}

fn sprintf(_interpreter: &mut Interpreter<'_>, mut arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "sprintf")?;
    let format = arguments
        .remove(0)
        .into_string()
        .context("sprintf format must be a string")?;
    let mut values = arguments.into_iter();
    let mut output = String::new();
    let mut chars = format.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '%' {
            output.push(ch);
            continue;
        }
        if chars.peek() == Some(&'%') {
            chars.next();
            output.push('%');
            continue;
        }
        let left_align = if chars.peek() == Some(&'-') {
            chars.next();
            true
        } else {
            false
        };
        let mut width = String::new();
        while chars.peek().is_some_and(char::is_ascii_digit) {
            width.push(chars.next().expect("peeked character"));
        }
        let specifier = chars.next().context("incomplete sprintf directive")?;
        let value = values.next().context("not enough sprintf arguments")?;
        let formatted = match specifier {
            's' => value.to_string(),
            'd' | 'i' => value
                .as_int()
                .context("sprintf %d requires an integer")?
                .to_string(),
            'f' => match value {
                LpcValue::Float(value) => value.to_string(),
                LpcValue::Int(value) => (value as f64).to_string(),
                _ => bail!("sprintf %f requires a number"),
            },
            'O' => value.lpc_repr(),
            other => bail!("unsupported sprintf directive %{other}"),
        };
        let width: usize = width.parse().unwrap_or(0);
        if formatted.len() >= width {
            output.push_str(&formatted);
        } else if left_align {
            output.push_str(&formatted);
            output.push_str(&" ".repeat(width - formatted.len()));
        } else {
            output.push_str(&" ".repeat(width - formatted.len()));
            output.push_str(&formatted);
        }
    }
    Ok(LpcValue::String(output))
}

fn atoi(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let value = string_argument(&arguments, 0, "atoi")?;
    Ok(LpcValue::Int(value.trim().parse().unwrap_or(0)))
}

fn to_string(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "to_string")?;
    Ok(LpcValue::String(arguments[0].to_string()))
}

fn type_of(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "typeof")?;
    Ok(LpcValue::String(arguments[0].type_name().to_owned()))
}

fn time(_interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    Ok(LpcValue::Int(seconds as i64))
}

fn debug_message(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "debug_message")?;
    tracing::info!(target: "mudlib", "{}", arguments[0]);
    Ok(LpcValue::Int(1))
}

fn shutdown(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    interpreter.world.request_shutdown();
    Ok(LpcValue::Int(1))
}

fn require(arguments: &[LpcValue], count: usize, name: &str) -> Result<()> {
    if arguments.len() < count {
        bail!("{name} expects at least {count} argument(s)");
    }
    Ok(())
}

fn string_argument<'a>(
    arguments: &'a [LpcValue],
    index: usize,
    name: &str,
) -> Result<&'a str> {
    arguments
        .get(index)
        .and_then(LpcValue::as_string)
        .with_context(|| format!("{name} argument {} must be a string", index + 1))
}

fn object_argument(value: &LpcValue, name: &str) -> Result<ObjectRef> {
    match value {
        LpcValue::Object(object) if !object.lock().destructed => Ok(object.clone()),
        _ => bail!("{name} requires a live object"),
    }
}

fn resolve_object(
    interpreter: &Interpreter<'_>,
    value: &LpcValue,
    load: bool,
) -> Result<Option<ObjectRef>> {
    match value {
        LpcValue::Object(object) if !object.lock().destructed => Ok(Some(object.clone())),
        LpcValue::String(path) if load => interpreter.world.load_object(path).map(Some),
        LpcValue::String(path) => Ok(interpreter.world.find_object(path)),
        LpcValue::Null => Ok(None),
        _ => bail!("expected an object or object path"),
    }
}

fn objects_from_value(value: &LpcValue) -> Vec<ObjectRef> {
    match value {
        LpcValue::Object(object) => vec![object.clone()],
        LpcValue::Array(values) => values.iter().flat_map(objects_from_value).collect(),
        _ => Vec::new(),
    }
}

fn deliver_room(room: &ObjectRef, message: &str, excludes: &[ObjectRef]) {
    let inventory = room.lock().inventory.clone();
    for object in inventory {
        if !excludes
            .iter()
            .any(|excluded| Arc::ptr_eq(excluded, &object))
        {
            object.lock().write(message.to_owned());
        }
    }
}

fn deliver_target(
    interpreter: &Interpreter<'_>,
    target: &LpcValue,
    message: &str,
    excludes: &[ObjectRef],
) -> Result<()> {
    match target {
        LpcValue::Object(object) => {
            if !excludes
                .iter()
                .any(|excluded| Arc::ptr_eq(excluded, object))
            {
                if !object.lock().write(message.to_owned()) {
                    deliver_room(object, message, excludes);
                }
            }
        }
        LpcValue::Array(values) => {
            for value in values {
                deliver_target(interpreter, value, message, excludes)?;
            }
        }
        LpcValue::String(path) => {
            if let Some(object) = interpreter.world.find_object(path) {
                deliver_room(&object, message, excludes);
            }
        }
        LpcValue::Null => {}
        _ => bail!("message target must be an object, object array, or path"),
    }
    Ok(())
}
