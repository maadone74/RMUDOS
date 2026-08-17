pub mod mudos_extra;

use crate::vm::interpret::Interpreter;
use crate::vm::object::{ObjectRef, PendingInput};
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
        functions.insert("filter_array", filter_array);
        functions.insert("map_array", map_array);
        functions.insert("evaluate", evaluate);
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
        functions.insert("origin", origin_efun);
        functions.insert("users", users);
        functions.insert("call_other", call_other);
        functions.insert("getuid", getuid);
        functions.insert("geteuid", geteuid);
        functions.insert("seteuid", seteuid);
        functions.insert("enable_commands", enable_commands);
        functions.insert("disable_commands", disable_commands);
        functions.insert("living", living_efun);
        functions.insert("interactive", interactive_efun);
        functions.insert("wizardp", wizardp);
        functions.insert("userp", userp);
        functions.insert("sprintf", sprintf);
        functions.insert("printf", printf);
        functions.insert("atoi", atoi);
        functions.insert("to_string", to_string);
        functions.insert("typeof", type_of);
        functions.insert("functionp", functionp);
        functions.insert("stringp", stringp);
        functions.insert("objectp", objectp);
        functions.insert("intp", intp);
        functions.insert("pointerp", pointerp);
        functions.insert("mapp", mapp);
        functions.insert("time", time);
        functions.insert("random", random_efun);
        functions.insert("debug_message", debug_message);
        functions.insert("shutdown", shutdown);
        functions.insert("set_heart_beat", set_heart_beat);
        functions.insert("query_heart_beat", query_heart_beat);
        functions.insert("input_to", input_to);
        functions.insert("new", clone_object);
        functions.insert("deep_inventory", deep_inventory);
        functions.insert("present", present);
        functions.insert("query_idle", query_idle);
        functions.insert("reset_eval_cost", reset_eval_cost);
        functions.insert("throw", throw_efun);
        functions.insert("nullp", nullp);
        functions.insert("undefinedp", nullp);
        functions.insert("master", master_efun);
        functions.insert("receive", receive);
        functions.insert("error", error_efun);
        functions.insert("function_exists", function_exists);
        functions.insert("export_uid", export_uid);
        mudos_extra::register(&mut functions);
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
    // After exec(), this_player may still be the login object (no socket) while
    // current_object is the interactive /std/user. Prefer an interactive recipient.
    let recipient = {
        let current = interpreter.current_object.clone();
        if current.lock().interactive.is_some() {
            current
        } else if let Some(player) = interpreter.this_player.clone() {
            if player.lock().interactive.is_some() {
                player
            } else {
                current
            }
        } else {
            current
        }
    };
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
    require(&arguments, 1, "lower_case")?;
    // MudOS mudlibs often pass 0/null; treat as empty string.
    let value = match &arguments[0] {
        LpcValue::String(text) => text.as_str(),
        LpcValue::Null | LpcValue::Int(0) => "",
        other => {
            bail!(
                "lower_case argument 1 must be a string (got {})",
                other.type_name()
            );
        }
    };
    Ok(LpcValue::String(value.to_lowercase()))
}

fn strlen(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::Int(
        string_argument(&arguments, 0, "strlen")?.chars().count() as i64,
    ))
}

fn explode(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "explode")?;
    // MudOS mudlibs often pass 0/null for missing crash-recovery data.
    let value = match &arguments[0] {
        LpcValue::String(s) => s.as_str(),
        LpcValue::Null | LpcValue::Int(0) => "",
        other => {
            bail!(
                "explode argument 1 must be a string (got {})",
                other.type_name()
            )
        }
    };
    let separator = match &arguments[1] {
        LpcValue::String(s) => s.as_str(),
        LpcValue::Null | LpcValue::Int(0) => "",
        other => {
            bail!(
                "explode argument 2 must be a string (got {})",
                other.type_name()
            )
        }
    };
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
        // MudOS-ish soft fail: treat bad containers as "not found".
        _ => None,
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
        // MudOS returns 0 for non-aggregates rather than erroring.
        _ => 0,
    };
    Ok(LpcValue::Int(size as i64))
}

pub(crate) fn filter_array(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "filter_array")?;
    let LpcValue::Array(values) = &arguments[0] else {
        bail!("filter_array first argument must be an array");
    };
    let fun = &arguments[1];
    // MudOS: filter_array(arr, "fun", ob, extras...) calls ob->fun(elem, extras...)
    let (target, extra): (Option<ObjectRef>, &[LpcValue]) = match (fun, arguments.get(2)) {
        (LpcValue::String(_), Some(LpcValue::Object(object))) => {
            (Some(object.clone()), arguments.get(3..).unwrap_or(&[]))
        }
        _ => (None, arguments.get(2..).unwrap_or(&[])),
    };
    let mut result = Vec::new();
    for item in values {
        let mut call_args = vec![item.clone()];
        call_args.extend_from_slice(extra);
        let value = match (fun, &target) {
            (LpcValue::String(name), Some(object)) => {
                interpreter.call_function(object.clone(), name, call_args)
            }
            _ => invoke_callable(interpreter, fun, call_args),
        }
        .context("filter_array callback failed")?;
        if value.is_truthy() {
            result.push(item.clone());
        }
    }
    Ok(LpcValue::Array(result))
}

pub(crate) fn map_array(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "map_array")?;
    let LpcValue::Array(values) = &arguments[0] else {
        bail!("map_array first argument must be an array");
    };
    let fun = &arguments[1];
    let (target, extra): (Option<ObjectRef>, &[LpcValue]) = match (fun, arguments.get(2)) {
        (LpcValue::String(_), Some(LpcValue::Object(object))) => {
            (Some(object.clone()), arguments.get(3..).unwrap_or(&[]))
        }
        _ => (None, arguments.get(2..).unwrap_or(&[])),
    };
    let mut result = Vec::new();
    for item in values {
        let mut call_args = vec![item.clone()];
        call_args.extend_from_slice(extra);
        let value = match (fun, &target) {
            (LpcValue::String(name), Some(object)) => {
                interpreter.call_function(object.clone(), name, call_args)
            }
            _ => invoke_callable(interpreter, fun, call_args),
        }
        .context("map_array callback failed")?;
        result.push(value);
    }
    Ok(LpcValue::Array(result))
}

fn evaluate(interpreter: &mut Interpreter<'_>, mut arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "evaluate")?;
    let fun = arguments.remove(0);
    invoke_callable(interpreter, &fun, arguments)
}

fn invoke_callable(
    interpreter: &mut Interpreter<'_>,
    fun: &LpcValue,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    match fun {
        LpcValue::Function(function) => interpreter.call_lpc_function(function, arguments),
        LpcValue::String(name) => {
            if Interpreter::find_function(&interpreter.current_object.lock().program, name)
                .is_some()
            {
                interpreter.call_function(interpreter.current_object.clone(), name, arguments)
            } else if let Some(efun) = interpreter.world.efuns.get(name.as_str()) {
                efun(interpreter, arguments)
            } else {
                bail!("unknown function {name}");
            }
        }
        _ => bail!("callback must be a function or string"),
    }
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

fn origin_efun(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::String(interpreter.origin.to_owned()))
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
    let target = match resolve_object(interpreter, &target_value, true)? {
        Some(object) => object,
        None => return Ok(LpcValue::Null),
    };
    interpreter.call_function(target, &function, arguments)
}

fn getuid(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::String(interpreter.current_object.lock().uid.clone()))
}

fn geteuid(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    if arguments.is_empty() {
        return Ok(LpcValue::String(
            interpreter.current_object.lock().euid.clone(),
        ));
    }
    let object = resolve_object(interpreter, &arguments[0], false)?
        .context("geteuid target does not exist")?;
    let euid = object.lock().euid.clone();
    Ok(LpcValue::String(euid))
}

fn seteuid(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "seteuid")?;
    let euid = arguments[0].to_string();
    // Soft-call master->valid_seteuid when present.
    if let Some(master) = interpreter.world.master() {
        let _ = interpreter.world.apply(
            master,
            "valid_seteuid",
            vec![
                LpcValue::Object(interpreter.current_object.clone()),
                LpcValue::String(euid.clone()),
            ],
            None,
            Some(interpreter.current_object.clone()),
        );
    }
    interpreter.current_object.lock().euid = euid;
    Ok(LpcValue::Int(1))
}

fn enable_commands(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    interpreter.current_object.lock().commands_enabled = true;
    Ok(LpcValue::Int(1))
}

fn disable_commands(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    interpreter.current_object.lock().commands_enabled = false;
    Ok(LpcValue::Int(1))
}

fn living_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "living")?;
    let living = match &arguments[0] {
        LpcValue::Object(object) => {
            let guard = object.lock();
            !guard.destructed && guard.living_name.is_some()
        }
        _ => false,
    };
    Ok(LpcValue::Int(i64::from(living)))
}

fn interactive_efun(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "interactive")?;
    Ok(LpcValue::Int(i64::from(matches!(
        &arguments[0],
        LpcValue::Object(object) if object.lock().interactive.is_some()
    ))))
}

fn wizardp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let object = if arguments.is_empty() {
        return Ok(LpcValue::Int(0));
    } else {
        match &arguments[0] {
            LpcValue::Object(object) => object.clone(),
            _ => return Ok(LpcValue::Int(0)),
        }
    };
    let wizard = object.lock().wizard;
    Ok(LpcValue::Int(i64::from(wizard)))
}

fn userp(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    if arguments.is_empty() {
        return Ok(LpcValue::Int(i64::from(
            interpreter.current_object.lock().interactive.is_some(),
        )));
    }
    interactive_efun(interpreter, arguments)
}

fn sprintf(_interpreter: &mut Interpreter<'_>, mut arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "sprintf")?;
    let format = arguments
        .remove(0)
        .into_string()
        .context("sprintf format must be a string")?;
    let mut values = arguments.into_iter().peekable();
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

        // MudOS modifiers may appear in any order before the type letter.
        #[derive(Clone, Copy, PartialEq, Eq)]
        enum Align {
            Right,
            Left,
            Center,
        }
        let mut align = Align::Right;
        let mut pad = " ".to_owned();
        let mut zero_pad = false;
        let mut show_plus = false;
        let mut space_positive = false;
        let mut width: Option<usize> = None;
        let mut precision: Option<usize> = None;

        loop {
            match chars.peek().copied() {
                Some('-') => {
                    chars.next();
                    align = Align::Left;
                }
                Some('|') => {
                    chars.next();
                    align = Align::Center;
                }
                Some('+') => {
                    chars.next();
                    show_plus = true;
                }
                Some(' ') => {
                    chars.next();
                    space_positive = true;
                }
                Some('0') if width.is_none() => {
                    chars.next();
                    zero_pad = true;
                    pad = "0".to_owned();
                }
                Some('\'') => {
                    chars.next();
                    let mut pad_chars = String::new();
                    while let Some(c) = chars.next() {
                        if c == '\'' {
                            break;
                        }
                        if c == '\\' {
                            if let Some(escaped) = chars.next() {
                                pad_chars.push(escaped);
                            }
                        } else {
                            pad_chars.push(c);
                        }
                    }
                    if !pad_chars.is_empty() {
                        pad = pad_chars;
                    }
                }
                Some('*') => {
                    chars.next();
                    let w = values
                        .next()
                        .and_then(|v| v.as_int())
                        .context("sprintf %* requires an integer width")?;
                    width = Some(w.max(0) as usize);
                }
                Some(c) if c.is_ascii_digit() => {
                    let mut digits = String::new();
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        digits.push(chars.next().expect("digit"));
                    }
                    width = Some(digits.parse().unwrap_or(0));
                }
                Some('.') => {
                    chars.next();
                    if chars.peek() == Some(&'*') {
                        chars.next();
                        let p = values
                            .next()
                            .and_then(|v| v.as_int())
                            .context("sprintf %.* requires an integer precision")?;
                        precision = Some(p.max(0) as usize);
                    } else {
                        let mut digits = String::new();
                        while chars.peek().is_some_and(char::is_ascii_digit) {
                            digits.push(chars.next().expect("digit"));
                        }
                        precision = Some(digits.parse().unwrap_or(0));
                    }
                }
                Some(':') => {
                    chars.next();
                    let mut digits = String::new();
                    while chars.peek().is_some_and(char::is_ascii_digit) {
                        digits.push(chars.next().expect("digit"));
                    }
                    let n: usize = digits.parse().unwrap_or(0);
                    width = Some(n);
                    precision = Some(n);
                }
                Some('=' | '#' | '$') => {
                    // Column/table/justify: ignored for simple formatting.
                    chars.next();
                }
                _ => break,
            }
        }

        let specifier = chars.next().context("incomplete sprintf directive")?;
        if zero_pad && pad == " " {
            pad = "0".to_owned();
        }

        let mut formatted = match specifier {
            's' => {
                let mut text = values
                    .next()
                    .context("not enough sprintf arguments")?
                    .to_string();
                if let Some(p) = precision {
                    text = text.chars().take(p).collect();
                }
                text
            }
            'c' => {
                let value = values.next().context("not enough sprintf arguments")?;
                match value {
                    LpcValue::Int(code) => char::from_u32(code as u32)
                        .unwrap_or('?')
                        .to_string(),
                    LpcValue::String(s) => s.chars().next().unwrap_or('?').to_string(),
                    _ => bail!("sprintf %c requires int or string"),
                }
            }
            'd' | 'i' => {
                let n = values
                    .next()
                    .context("not enough sprintf arguments")?
                    .as_int()
                    .context("sprintf %d requires an integer")?;
                let mut text = n.to_string();
                if n >= 0 {
                    if show_plus {
                        text = format!("+{text}");
                    } else if space_positive {
                        text = format!(" {text}");
                    }
                }
                text
            }
            'f' => {
                let value = values.next().context("not enough sprintf arguments")?;
                match value {
                    LpcValue::Float(v) => {
                        if let Some(p) = precision {
                            format!("{v:.p$}")
                        } else {
                            v.to_string()
                        }
                    }
                    LpcValue::Int(v) => {
                        let v = v as f64;
                        if let Some(p) = precision {
                            format!("{v:.p$}")
                        } else {
                            v.to_string()
                        }
                    }
                    _ => bail!("sprintf %f requires a number"),
                }
            }
            'O' => values
                .next()
                .context("not enough sprintf arguments")?
                .lpc_repr(),
            other => bail!("unsupported sprintf directive %{other}"),
        };

        if let Some(w) = width {
            if formatted.chars().count() < w {
                let pad_len = w - formatted.chars().count();
                let pad_unit = if pad.is_empty() { " " } else { pad.as_str() };
                let make_pad = |n: usize| -> String {
                    if n == 0 {
                        return String::new();
                    }
                    let mut out = String::new();
                    while out.chars().count() < n {
                        out.push_str(pad_unit);
                    }
                    out.chars().take(n).collect()
                };
                formatted = match align {
                    Align::Left => format!("{}{}", formatted, make_pad(pad_len)),
                    Align::Right => format!("{}{}", make_pad(pad_len), formatted),
                    Align::Center => {
                        let left = pad_len / 2;
                        let right = pad_len - left;
                        format!("{}{}{}", make_pad(left), formatted, make_pad(right))
                    }
                };
            }
        }
        output.push_str(&formatted);
    }
    Ok(LpcValue::String(output))
}

fn printf(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let formatted = sprintf(interpreter, arguments)?;
    write(interpreter, vec![formatted])
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

fn functionp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "functionp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::Function(_)
    ))))
}

fn stringp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "stringp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::String(_)
    ))))
}

fn objectp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "objectp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        &arguments[0],
        LpcValue::Object(object) if !object.lock().destructed
    ))))
}

fn intp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "intp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::Int(_)
    ))))
}

fn pointerp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "pointerp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::Array(_)
    ))))
}

fn mapp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "mapp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::Mapping(_)
    ))))
}

fn time(_interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock is before the Unix epoch")?
        .as_secs();
    Ok(LpcValue::Int(seconds as i64))
}

/// MudOS `random(n)` — integer in `[0, n)`.
fn random_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "random")?;
    let n = arguments[0]
        .as_int()
        .context("random requires an integer")?;
    if n <= 0 {
        return Ok(LpcValue::Int(0));
    }
    thread_local! {
        static STATE: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
    }
    let value = STATE.with(|state| {
        let mut s = state.get();
        if s == 0 {
            s = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            s ^= s << 13;
        }
        // Numerical Recipes LCG
        s = s.wrapping_mul(1664525).wrapping_add(1013904223);
        state.set(s);
        (s % (n as u64)) as i64
    });
    Ok(LpcValue::Int(value))
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

fn set_heart_beat(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "set_heart_beat")?;
    let flag = arguments[0].as_int().unwrap_or(0);
    interpreter.current_object.lock().heart_beat = flag as i32;
    Ok(LpcValue::Int(1))
}

fn query_heart_beat(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = if arguments.is_empty() {
        interpreter.current_object.clone()
    } else {
        object_argument(&arguments[0], "query_heart_beat")?
    };
    let beat = object.lock().heart_beat;
    Ok(LpcValue::Int(i64::from(beat)))
}

fn input_to(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "input_to")?;
    let fun = arguments[0].clone();
    if !matches!(fun, LpcValue::String(_) | LpcValue::Function(_)) {
        bail!("input_to requires a string or function callback");
    }
    // MudOS: input_to(fun), input_to(fun, flag), input_to(fun, flag, args...)
    let flag = arguments
        .get(1)
        .and_then(LpcValue::as_int)
        .unwrap_or(0);
    let no_echo = flag & 1 != 0;
    let extra = if arguments.len() <= 1 {
        Vec::new()
    } else if arguments.len() == 2 && matches!(arguments[1], LpcValue::Int(_) | LpcValue::Null) {
        Vec::new()
    } else if arguments.len() >= 2 && matches!(arguments[1], LpcValue::Int(_) | LpcValue::Null) {
        arguments[2..].to_vec()
    } else {
        arguments[1..].to_vec()
    };
    // MudOS binds input_to to the interactive player (command giver), not
    // necessarily current_object (e.g. room code calling input_to during pick).
    let owner = interpreter.current_object.clone();
    let target = {
        let from_player = interpreter.this_player.clone().filter(|p| {
            !p.lock().destructed && p.lock().interactive.is_some()
        });
        let from_current = (!owner.lock().destructed && owner.lock().interactive.is_some())
            .then(|| owner.clone());
        from_player
            .or(from_current)
            .or_else(|| interpreter.this_player.clone())
            .unwrap_or_else(|| owner.clone())
    };
    {
        let mut object = target.lock();
        if no_echo {
            let _ = object.set_echo(false);
        }
        object.pending_input = Some(PendingInput {
            owner,
            fun,
            extra,
            no_echo,
        });
    }
    Ok(LpcValue::Int(1))
}

fn deep_inventory(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "deep_inventory")?;
    let root = object_argument(&arguments[0], "deep_inventory")?;
    let mut result = Vec::new();
    let mut stack = root.lock().inventory.clone();
    while let Some(object) = stack.pop() {
        if object.lock().destructed {
            continue;
        }
        let children = object.lock().inventory.clone();
        result.push(LpcValue::Object(object));
        stack.extend(children);
    }
    Ok(LpcValue::Array(result))
}

fn present(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "present")?;
    let needle = &arguments[0];
    let container = if arguments.len() >= 2 {
        resolve_object(interpreter, &arguments[1], false)?
            .context("present container does not exist")?
    } else {
        interpreter
            .this_player
            .clone()
            .unwrap_or_else(|| interpreter.current_object.clone())
    };
    let inventory = container.lock().inventory.clone();
    for object in inventory {
        if object.lock().destructed {
            continue;
        }
        match needle {
            LpcValue::Object(target) if Arc::ptr_eq(target, &object) => {
                return Ok(LpcValue::Object(object));
            }
            LpcValue::String(id) => {
                let name = object.lock().file_name();
                if name == *id || name.ends_with(id.as_str()) {
                    return Ok(LpcValue::Object(object));
                }
                let id_result = interpreter.world.apply(
                    object.clone(),
                    "id",
                    vec![LpcValue::String(id.clone())],
                    interpreter.this_player.clone(),
                    Some(interpreter.current_object.clone()),
                )?;
                if id_result.is_truthy() {
                    return Ok(LpcValue::Object(object));
                }
            }
            _ => {}
        }
    }
    Ok(LpcValue::Null)
}

fn query_idle(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    // Stub until idle tracking exists.
    let _ = arguments;
    Ok(LpcValue::Int(0))
}

fn reset_eval_cost(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    interpreter.reset_cost();
    Ok(LpcValue::Int(1))
}

fn throw_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "throw")?;
    bail!("{}", arguments[0]);
}

fn nullp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "nullp")?;
    Ok(LpcValue::Int(i64::from(matches!(
        arguments[0],
        LpcValue::Null
    ))))
}

fn master_efun(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(interpreter
        .world
        .master()
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null))
}

fn receive(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "receive")?;
    let message = arguments[0].to_string();
    let ok = interpreter.current_object.lock().write(message);
    Ok(LpcValue::Int(i64::from(ok)))
}

fn error_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "error")?;
    bail!("{}", arguments[0]);
}

fn function_exists(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "function_exists")?;
    let name = arguments[0]
        .as_string()
        .context("function_exists requires a function name")?;
    let object = if arguments.len() >= 2 {
        object_argument(&arguments[1], "function_exists")?
    } else {
        interpreter.current_object.clone()
    };
    let program = object.lock().program.clone();
    let found = Interpreter::find_function(&program, name).is_some();
    Ok(if found {
        LpcValue::String(object.lock().file_name())
    } else {
        LpcValue::Null
    })
}

fn export_uid(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    // MudOS export_uid(ob) — set ob's uid to current euid when allowed.
    require(&arguments, 1, "export_uid")?;
    let target = object_argument(&arguments[0], "export_uid")?;
    let euid = interpreter.current_object.lock().euid.clone();
    target.lock().uid = euid;
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
        LpcValue::String(path) if path.is_empty() || path == "0" => Ok(None),
        LpcValue::String(path) if load => match interpreter.world.load_object(path) {
            Ok(object) => Ok(Some(object)),
            Err(_) => Ok(None),
        },
        LpcValue::String(path) => Ok(interpreter.world.find_object(path)),
        LpcValue::Null | LpcValue::Int(0) => Ok(None),
        // Soft: MudOS call_other on non-objects returns 0.
        _ => Ok(None),
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
