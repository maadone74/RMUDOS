//! Additional MudOS-oriented efuns (filesystem, call_out, commands, login helpers).

use crate::config::normalize_object_path;
use crate::vm::interpret::Interpreter;
use crate::vm::object::{Action, ObjectRef};
use crate::vm::program::Program;
use crate::vm::value::LpcValue;
use anyhow::{bail, Context, Result};
use indexmap::IndexMap;
use std::fs;
use std::io::Write as IoWrite;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

pub fn register(functions: &mut IndexMap<&'static str, super::EfunFn>) {
    functions.insert("call_out", call_out);
    functions.insert("remove_call_out", remove_call_out);
    functions.insert("find_call_out", find_call_out);
    functions.insert("read_file", read_file);
    functions.insert("write_file", write_file);
    functions.insert("file_size", file_size);
    functions.insert("file_exists", file_exists);
    // Prefer mudlib simul_efun log_file (seteuid UID_LOG) over a raw efun.
    functions.insert("get_dir", get_dir);
    functions.insert("mkdir", mkdir_efun);
    functions.insert("rm", rm_efun);
    functions.insert("cp", cp_efun);
    functions.insert("rename", rename_efun);
    functions.insert("read_database", read_database);
    functions.insert("sscanf", sscanf);
    functions.insert("sscanf_values", sscanf_values);
    functions.insert("replace_string", replace_string);
    functions.insert("strsrch", strsrch);
    functions.insert("to_int", to_int_efun);
    functions.insert("to_float", to_float_efun);
    functions.insert("pow", pow_efun);
    functions.insert("sqrt", sqrt_efun);
    functions.insert("sin", sin_efun);
    functions.insert("cos", cos_efun);
    functions.insert("tan", tan_efun);
    functions.insert("asin", asin_efun);
    functions.insert("acos", acos_efun);
    functions.insert("atan", atan_efun);
    functions.insert("log", log_efun);
    functions.insert("exp", exp_efun);
    functions.insert("floor", floor_efun);
    functions.insert("ceil", ceil_efun);
    functions.insert("ctime", ctime);
    functions.insert("allocate", allocate);
    functions.insert("allocate_mapping", allocate_mapping);
    functions.insert("map_delete", map_delete);
    functions.insert("copy", copy_efun);
    functions.insert("sort_array", sort_array);
    functions.insert("base_name", base_name);
    functions.insert("regexp", regexp_efun);
    functions.insert("add_action", add_action);
    functions.insert("clear_actions", clear_actions);
    functions.insert("notify_fail", notify_fail);
    functions.insert("query_verb", query_verb);
    functions.insert("command", command_efun);
    functions.insert("commands", commands_efun);
    functions.insert("exec", exec_efun);
    functions.insert("crypt", crypt_efun);
    functions.insert("user_exists", user_exists);
    functions.insert("find_player", find_player);
    functions.insert("find_living", find_living);
    functions.insert("set_living_name", set_living_name);
    functions.insert("enable_wizard", enable_wizard);
    functions.insert("version", version_efun);
    functions.insert("mud_name", mud_name);
    functions.insert("mudlib", mudlib_efun);
    functions.insert("mudlib_version", mudlib_version);
    functions.insert("query_ip_number", query_ip_number);
    functions.insert("query_ip_name", query_ip_name);
    functions.insert("save_object", save_object);
    functions.insert("restore_object", restore_object);
    functions.insert("save_variable", save_variable);
    functions.insert("restore_variable", restore_variable);
    functions.insert("shadow", shadow_efun);
    functions.insert("query_shadowing", query_shadowing);
    functions.insert("next_shadow", next_shadow);
    functions.insert("bind", bind_efun);
    functions.insert("map", map_alias);
    functions.insert("filter", filter_alias);
    functions.insert("map_mapping", map_mapping);
    functions.insert("filter_mapping", filter_mapping);
    functions.insert("unique_array", unique_array);
    functions.insert("children", children_efun);
    functions.insert("objects", objects_efun);
    functions.insert("livings", livings_efun);
    functions.insert("uptime", uptime_efun);
    functions.insert("localtime", localtime_efun);
    functions.insert("remove_action", remove_action);
    functions.insert("virtualp", virtualp);
    functions.insert("parse_command", parse_command);
    functions.insert("parse_command_values", parse_command_values);
    functions.insert("set_hide", set_hide);
    functions.insert("snoop", snoop_efun);
    functions.insert("query_snoop", query_snoop);
    functions.insert("query_snooping", query_snooping);
    functions.insert("inherits", inherits_efun);
    functions.insert("deep_inherit_list", deep_inherit_list);
    functions.insert("call_out_info", call_out_info);
    functions.insert("stat", stat_efun);
    functions.insert("ed", ed_efun);
    functions.insert("in_edit", in_edit);
    functions.insert("in_input", in_input);
    // Socket efuns: stubs until real MudOS sockets are implemented.
    // Network daemons call these from call_out("setup"); returning EESOCKET
    // lets them soft-fail instead of aborting with "unknown efun".
    functions.insert("socket_create", socket_create);
    functions.insert("socket_bind", socket_unsupported);
    functions.insert("socket_listen", socket_unsupported);
    functions.insert("socket_accept", socket_unsupported);
    functions.insert("socket_connect", socket_unsupported);
    functions.insert("socket_write", socket_unsupported);
    functions.insert("socket_close", socket_close_stub);
    functions.insert("socket_release", socket_unsupported);
    functions.insert("socket_acquire", socket_unsupported);
    functions.insert("socket_address", socket_address_stub);
    functions.insert("socket_error", socket_error_stub);
    functions.insert("dump_socket_status", dump_socket_status);
}

fn require(arguments: &[LpcValue], count: usize, name: &str) -> Result<()> {
    if arguments.len() < count {
        bail!("{name} expects at least {count} argument(s)");
    }
    Ok(())
}

fn resolve_mudlib_path(interpreter: &Interpreter<'_>, path: &str) -> Result<PathBuf> {
    let path = path.replace('\\', "/");
    let relative = path.trim_start_matches('/');
    let mudlib = interpreter.world.config.mudlib.clone();
    let mut resolved = mudlib.clone();
    for component in Path::new(relative).components() {
        match component {
            Component::Normal(part) => resolved.push(part),
            Component::CurDir => {}
            Component::ParentDir => {
                if !resolved.pop() || !resolved.starts_with(&mudlib) {
                    bail!("path escapes mudlib root");
                }
            }
            _ => bail!("invalid path component"),
        }
    }
    // Do not canonicalize: on OneDrive / cloud paths canonicalize() can hang.
    // Component walking above already keeps the path under mudlib.
    if !resolved.starts_with(&mudlib) {
        bail!("path escapes mudlib root");
    }
    Ok(resolved)
}

fn check_valid_access(
    interpreter: &mut Interpreter<'_>,
    path: &str,
    fun: &str,
    write: bool,
) -> Result<()> {
    thread_local! {
        static DEPTH: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
    }
    // Avoid recursion when master valid_* itself touches the filesystem.
    let depth = DEPTH.with(|d| {
        let next = d.get().saturating_add(1);
        d.set(next);
        next
    });
    if depth > 1 {
        DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
        return Ok(());
    }
    let result = (|| {
        let Some(master) = interpreter.world.master() else {
            return Ok(());
        };
        let apply_name = if write { "valid_write" } else { "valid_read" };
        let result = interpreter.world.apply(
            master,
            apply_name,
            vec![
                LpcValue::String(path.to_owned()),
                LpcValue::Object(interpreter.current_object.clone()),
                LpcValue::String(fun.to_owned()),
            ],
            None,
            Some(interpreter.current_object.clone()),
        )?;
        if matches!(result, LpcValue::Null) || result.is_truthy() {
            return Ok(());
        }
        bail!("permission denied: {apply_name}({path})");
    })();
    DEPTH.with(|d| d.set(d.get().saturating_sub(1)));
    result
}

fn call_out(interpreter: &mut Interpreter<'_>, mut arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "call_out")?;
    let fun = arguments.remove(0);
    let delay = arguments.remove(0).as_int().unwrap_or(0) as f64;
    if !matches!(fun, LpcValue::String(_) | LpcValue::Function(_)) {
        bail!("call_out requires a string or function");
    }
    let id = interpreter.world.call_outs.schedule(
        interpreter.current_object.clone(),
        fun,
        delay,
        arguments,
    );
    Ok(LpcValue::Int(id as i64))
}

fn remove_call_out(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "remove_call_out")?;
    let id = arguments[0].as_int().unwrap_or(0);
    Ok(LpcValue::Int(interpreter.world.call_outs.remove(id)))
}

fn find_call_out(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "find_call_out")?;
    let name = arguments[0]
        .as_string()
        .context("find_call_out requires a function name")?;
    Ok(LpcValue::Int(interpreter.world.call_outs.find(
        &interpreter.current_object,
        name,
    )))
}

fn read_file(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "read_file")?;
    let path = arguments[0]
        .as_string()
        .context("read_file requires a path")?;
    check_valid_access(interpreter, path, "read_file", false)?;
    let resolved = resolve_mudlib_path(interpreter, path)?;
    match fs::read_to_string(&resolved) {
        Ok(contents) => {
            let start = arguments.get(1).and_then(LpcValue::as_int).unwrap_or(0);
            let number = arguments.get(2).and_then(LpcValue::as_int).unwrap_or(0);
            if start > 0 || number > 0 {
                let lines: Vec<&str> = contents.lines().collect();
                let start_idx = if start > 0 { (start as usize).saturating_sub(1) } else { 0 };
                let end_idx = if number > 0 {
                    (start_idx + number as usize).min(lines.len())
                } else {
                    lines.len()
                };
                Ok(LpcValue::String(lines[start_idx..end_idx].join("\n")))
            } else {
                Ok(LpcValue::String(contents))
            }
        }
        Err(_) => Ok(LpcValue::Null),
    }
}

fn write_file(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "write_file")?;
    let path = arguments[0]
        .as_string()
        .context("write_file requires a path")?;
    check_valid_access(interpreter, path, "write_file", true)?;
    let contents = arguments[1].to_string();
    let append = arguments.get(2).map(LpcValue::is_truthy).unwrap_or(false);
    let resolved = resolve_mudlib_path(interpreter, path)?;
    if let Some(parent) = resolved.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let result = if append {
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&resolved)
            .and_then(|mut file| file.write_all(contents.as_bytes()))
    } else {
        fs::write(&resolved, contents.as_bytes())
    };
    if result.is_ok() {
        interpreter.world.fs_cache.invalidate(&resolved);
    }
    Ok(LpcValue::Int(i64::from(result.is_ok())))
}

fn file_size(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "file_size")?;
    let path = arguments[0]
        .as_string()
        .context("file_size requires a path")?;
    let started = Instant::now();
    tracing::info!(path, "file_size start");
    if check_valid_access(interpreter, path, "file_size", false).is_err() {
        tracing::info!(
            path,
            elapsed_ms = started.elapsed().as_millis(),
            "file_size denied"
        );
        return Ok(LpcValue::Int(-1));
    }
    let resolved = resolve_mudlib_path(interpreter, path)?;
    let size = interpreter.world.fs_cache.stat(&resolved).file_size();
    tracing::info!(
        path,
        size,
        elapsed_ms = started.elapsed().as_millis(),
        "file_size done"
    );
    Ok(LpcValue::Int(size))
}

fn file_exists(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "file_exists")?;
    let path = arguments[0]
        .as_string()
        .context("file_exists requires a path")?;
    let resolved = resolve_mudlib_path(interpreter, path)?;
    Ok(LpcValue::Int(i64::from(
        interpreter.world.fs_cache.stat(&resolved).exists(),
    )))
}

fn get_dir(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "get_dir")?;
    let path = arguments[0]
        .as_string()
        .context("get_dir requires a path")?
        .replace('\\', "/");
    let detailed = arguments.get(1).and_then(LpcValue::as_int) == Some(-1);
    let started = Instant::now();
    tracing::info!(path = %path, detailed, "get_dir start");
    let result = get_dir_inner(interpreter, &path, detailed)?;
    let n = match &result {
        LpcValue::Array(items) => items.len(),
        _ => 0,
    };
    tracing::info!(
        path = %path,
        entries = n,
        elapsed_ms = started.elapsed().as_millis(),
        "get_dir done"
    );
    Ok(result)
}

fn get_dir_inner(
    interpreter: &mut Interpreter<'_>,
    path: &str,
    detailed: bool,
) -> Result<LpcValue> {
    match parse_get_dir_query(path) {
        GetDirQuery::List { dir, pattern } => {
            if check_valid_access(interpreter, &dir, "get_dir", false).is_err() {
                return Ok(LpcValue::Null);
            }
            let resolved = resolve_mudlib_path(interpreter, &dir)?;
            if !interpreter.world.fs_cache.stat(&resolved).is_dir() {
                return Ok(LpcValue::Null);
            }
            let Some(names) = interpreter.world.fs_cache.list_dir(&resolved) else {
                return Ok(LpcValue::Null);
            };
            let mut matched: Vec<String> = names
                .into_iter()
                .filter(|name| {
                    pattern
                        .as_deref()
                        .is_none_or(|pat| mudos_glob_match(pat, name))
                })
                .collect();
            matched.sort();
            Ok(LpcValue::Array(if detailed {
                matched
                    .into_iter()
                    .map(|name| {
                        let child = resolved.join(&name);
                        let stat = interpreter.world.fs_cache.stat(&child);
                        LpcValue::Array(vec![
                            LpcValue::String(name),
                            LpcValue::Int(stat.file_size()),
                            LpcValue::Int(stat.mtime()),
                        ])
                    })
                    .collect()
            } else {
                matched.into_iter().map(LpcValue::String).collect()
            }))
        }
        GetDirQuery::File { path: file_path, name } => {
            if check_valid_access(interpreter, &file_path, "get_dir", false).is_err() {
                return Ok(LpcValue::Null);
            }
            let resolved = resolve_mudlib_path(interpreter, &file_path)?;
            let stat = interpreter.world.fs_cache.stat(&resolved);
            if stat.is_dir() {
                return get_dir_inner(
                    interpreter,
                    &format!("{}/", file_path.trim_end_matches('/')),
                    detailed,
                );
            }
            if !stat.exists() {
                return Ok(LpcValue::Null);
            }
            Ok(LpcValue::Array(vec![if detailed {
                LpcValue::Array(vec![
                    LpcValue::String(name),
                    LpcValue::Int(stat.file_size()),
                    LpcValue::Int(stat.mtime()),
                ])
            } else {
                LpcValue::String(name)
            }]))
        }
    }
}

enum GetDirQuery {
    List {
        dir: String,
        pattern: Option<String>,
    },
    File {
        path: String,
        name: String,
    },
}

/// MudOS: trailing `/` lists a directory; `*`/`?` match the last component;
/// a bare directory path lists it; a bare file path returns that name.
fn parse_get_dir_query(path: &str) -> GetDirQuery {
    let path = if path.is_empty() { "/" } else { path };
    if path.ends_with('/') {
        let dir = path.trim_end_matches('/');
        return GetDirQuery::List {
            dir: if dir.is_empty() {
                "/".to_owned()
            } else {
                dir.to_owned()
            },
            pattern: None,
        };
    }
    let (dir, name) = match path.rsplit_once('/') {
        Some(("", name)) => ("/", name),
        Some((dir, name)) => (dir, name),
        None => ("/", path.trim_start_matches('/')),
    };
    if name.contains('*') || name.contains('?') {
        GetDirQuery::List {
            dir: dir.to_owned(),
            pattern: Some(name.to_owned()),
        }
    } else {
        GetDirQuery::File {
            path: path.to_owned(),
            name: name.to_owned(),
        }
    }
}

fn mudos_glob_match(pattern: &str, name: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let name: Vec<char> = name.chars().collect();
    fn match_at(pattern: &[char], name: &[char]) -> bool {
        let mut pi = 0;
        let mut ni = 0;
        while pi < pattern.len() {
            match pattern[pi] {
                '*' => {
                    pi += 1;
                    if pi == pattern.len() {
                        return true;
                    }
                    while ni <= name.len() {
                        if match_at(&pattern[pi..], &name[ni..]) {
                            return true;
                        }
                        if ni == name.len() {
                            break;
                        }
                        ni += 1;
                    }
                    return false;
                }
                '?' => {
                    if ni >= name.len() {
                        return false;
                    }
                    pi += 1;
                    ni += 1;
                }
                ch => {
                    if ni >= name.len() || name[ni] != ch {
                        return false;
                    }
                    pi += 1;
                    ni += 1;
                }
            }
        }
        ni == name.len()
    }
    match_at(&pattern, &name)
}

fn mkdir_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "mkdir")?;
    let path = arguments[0].as_string().context("mkdir requires a path")?;
    let resolved = resolve_mudlib_path(interpreter, path)?;
    let ok = fs::create_dir_all(&resolved).is_ok();
    if ok {
        interpreter.world.fs_cache.invalidate(&resolved);
    }
    Ok(LpcValue::Int(i64::from(ok)))
}

fn rm_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "rm")?;
    let path = arguments[0].as_string().context("rm requires a path")?;
    let resolved = resolve_mudlib_path(interpreter, path)?;
    let ok = if interpreter.world.fs_cache.stat(&resolved).is_dir() {
        fs::remove_dir_all(&resolved).is_ok()
    } else {
        fs::remove_file(&resolved).is_ok()
    };
    if ok {
        interpreter.world.fs_cache.invalidate(&resolved);
    }
    Ok(LpcValue::Int(i64::from(ok)))
}

fn cp_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "cp")?;
    let from = arguments[0].as_string().context("cp from")?;
    let to = arguments[1].as_string().context("cp to")?;
    let src = resolve_mudlib_path(interpreter, from)?;
    let dst = resolve_mudlib_path(interpreter, to)?;
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ok = fs::copy(&src, &dst).is_ok();
    if ok {
        interpreter.world.fs_cache.invalidate(&dst);
    }
    Ok(LpcValue::Int(i64::from(ok)))
}

fn rename_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "rename")?;
    let from = arguments[0].as_string().context("rename from")?;
    let to = arguments[1].as_string().context("rename to")?;
    let src = resolve_mudlib_path(interpreter, from)?;
    let dst = resolve_mudlib_path(interpreter, to)?;
    if let Some(parent) = dst.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let ok = fs::rename(&src, &dst).is_ok();
    if ok {
        interpreter.world.fs_cache.invalidate(&src);
        interpreter.world.fs_cache.invalidate(&dst);
    }
    Ok(LpcValue::Int(i64::from(ok)))
}

fn read_database(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "read_database")?;
    let path = arguments[0]
        .as_string()
        .context("read_database requires a path")?;
    let resolved = resolve_mudlib_path(interpreter, path)?;
    if !resolved.is_file() {
        return Ok(LpcValue::Null);
    }
    let Ok(contents) = fs::read_to_string(&resolved) else {
        return Ok(LpcValue::Array(Vec::new()));
    };
    if contents.is_empty() {
        return Ok(LpcValue::Array(Vec::new()));
    }
    let lines = contents
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            !trimmed.is_empty() && !trimmed.starts_with('#')
        })
        .map(|line| LpcValue::String(line.to_owned()))
        .collect();
    Ok(LpcValue::Array(lines))
}

fn sscanf(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "sscanf")?;
    let input = arguments[0].as_string().unwrap_or("").to_owned();
    let format = arguments[1].as_string().unwrap_or("").to_owned();
    let captures = parse_sscanf(&input, &format);
    Ok(LpcValue::Int(captures.len() as i64))
}

fn sscanf_values(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "sscanf_values")?;
    let input = arguments[0].as_string().unwrap_or("").to_owned();
    let format = arguments[1].as_string().unwrap_or("").to_owned();
    Ok(LpcValue::Array(parse_sscanf(&input, &format)))
}

fn parse_sscanf(input: &str, format: &str) -> Vec<LpcValue> {
    let mut captures = Vec::new();
    let mut input_pos = 0;
    let format_bytes = format.as_bytes();
    let mut fi = 0;
    while fi < format_bytes.len() {
        if format_bytes[fi] == b'%' {
            fi += 1;
            let spec = format_bytes.get(fi).copied().unwrap_or(b's');
            fi += 1;
            match spec {
                b'%' => {
                    if input.as_bytes().get(input_pos) != Some(&b'%') {
                        return captures;
                    }
                    input_pos += 1;
                }
                b'*' => {
                    // `%*s` / `%*d` — parse but do not capture.
                    let inner = format_bytes.get(fi).copied().unwrap_or(b's');
                    fi += 1;
                    let dummy = parse_one_spec(input, &mut input_pos, inner, format_bytes, &mut fi);
                    if dummy.is_none() {
                        return captures;
                    }
                }
                b'd' | b'i' | b'f' | b's' => {
                    if let Some(value) =
                        parse_one_spec(input, &mut input_pos, spec, format_bytes, &mut fi)
                    {
                        captures.push(value);
                    } else {
                        return captures;
                    }
                }
                _ => {}
            }
        } else {
            if input.as_bytes().get(input_pos) != Some(&format_bytes[fi]) {
                return captures;
            }
            input_pos += 1;
            fi += 1;
        }
    }
    captures
}

fn parse_one_spec(
    input: &str,
    input_pos: &mut usize,
    spec: u8,
    format_bytes: &[u8],
    fi: &mut usize,
) -> Option<LpcValue> {
    match spec {
        b'd' | b'i' => {
            let rest = &input[*input_pos..];
            let chars: Vec<char> = rest.chars().collect();
            let mut end = 0;
            if chars.first() == Some(&'-') {
                end = 1;
            }
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            let num: String = chars[..end].iter().collect();
            let value = num.parse::<i64>().ok()?;
            *input_pos += num.len();
            Some(LpcValue::Int(value))
        }
        b'f' => {
            let rest = &input[*input_pos..];
            let chars: Vec<char> = rest.chars().collect();
            let mut end = 0;
            while end < chars.len()
                && (chars[end].is_ascii_digit() || chars[end] == '.' || chars[end] == '-')
            {
                end += 1;
            }
            let num: String = chars[..end].iter().collect();
            let value = num.parse::<f64>().ok()?;
            *input_pos += num.len();
            Some(LpcValue::Float(value))
        }
        b's' => {
            let mut literal = String::new();
            while *fi < format_bytes.len() && format_bytes[*fi] != b'%' {
                literal.push(format_bytes[*fi] as char);
                *fi += 1;
            }
            let rest = &input[*input_pos..];
            let value = if literal.is_empty() {
                *input_pos = input.len();
                rest.to_owned()
            } else if let Some(idx) = rest.find(&literal) {
                let before = rest[..idx].to_owned();
                *input_pos += idx + literal.len();
                before
            } else {
                return None;
            };
            Some(LpcValue::String(value))
        }
        _ => None,
    }
}

fn replace_string(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 3, "replace_string")?;
    let mut text = arguments[0].to_string();
    let from = arguments[1].to_string();
    let to = arguments[2].to_string();
    if from.is_empty() {
        return Ok(LpcValue::String(text));
    }
    let max = arguments.get(3).and_then(LpcValue::as_int).unwrap_or(0);
    if max <= 0 {
        text = text.replace(&from, &to);
    } else {
        for _ in 0..max {
            if let Some(idx) = text.find(&from) {
                text.replace_range(idx..idx + from.len(), &to);
            } else {
                break;
            }
        }
    }
    Ok(LpcValue::String(text))
}

fn strsrch(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "strsrch")?;
    let haystack = match &arguments[0] {
        LpcValue::String(s) => s.as_str(),
        LpcValue::Null | LpcValue::Int(0) => return Ok(LpcValue::Int(-1)),
        _ => return Ok(LpcValue::Int(-1)),
    };
    let needle = match &arguments[1] {
        LpcValue::String(s) => s.clone(),
        LpcValue::Int(ch) => char::from_u32(*ch as u32)
            .map(|c| c.to_string())
            .unwrap_or_default(),
        LpcValue::Null => return Ok(LpcValue::Int(-1)),
        _ => return Ok(LpcValue::Int(-1)),
    };
    if needle.is_empty() {
        return Ok(LpcValue::Int(-1));
    }
    let from_end = arguments.get(2).and_then(LpcValue::as_int) == Some(-1);
    let idx = if from_end {
        haystack.rfind(&needle)
    } else {
        haystack.find(&needle)
    };
    Ok(LpcValue::Int(idx.map(|i| i as i64).unwrap_or(-1)))
}

fn to_int_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "to_int")?;
    let value = match &arguments[0] {
        LpcValue::Int(v) => *v,
        LpcValue::Float(v) => *v as i64,
        LpcValue::String(s) => s
            .trim()
            .parse::<i64>()
            .or_else(|_| {
                s.trim()
                    .parse::<f64>()
                    .map(|f| f as i64)
                    .map_err(|_| anyhow::anyhow!("bad string"))
            })
            .unwrap_or(0),
        LpcValue::Null => 0,
        other => other.as_int().unwrap_or(0),
    };
    Ok(LpcValue::Int(value))
}

fn to_float_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "to_float")?;
    let value = match &arguments[0] {
        LpcValue::Float(v) => *v,
        LpcValue::Int(v) => *v as f64,
        LpcValue::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        LpcValue::Null => 0.0,
        _ => 0.0,
    };
    Ok(LpcValue::Float(value))
}

fn as_float_arg(value: &LpcValue) -> f64 {
    match value {
        LpcValue::Float(v) => *v,
        LpcValue::Int(v) => *v as f64,
        LpcValue::String(s) => s.trim().parse::<f64>().unwrap_or(0.0),
        LpcValue::Null => 0.0,
        _ => value.as_int().unwrap_or(0) as f64,
    }
}

fn pow_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "pow")?;
    let base = as_float_arg(&arguments[0]);
    let exp = as_float_arg(&arguments[1]);
    let result = base.powf(exp);
    if result.is_finite() {
        Ok(LpcValue::Float(result))
    } else {
        Ok(LpcValue::Float(0.0))
    }
}

fn sqrt_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "sqrt")?;
    let value = as_float_arg(&arguments[0]);
    if value < 0.0 {
        Ok(LpcValue::Float(0.0))
    } else {
        Ok(LpcValue::Float(value.sqrt()))
    }
}

fn float_unary(
    name: &str,
    arguments: &[LpcValue],
    f: impl FnOnce(f64) -> f64,
) -> Result<LpcValue> {
    require(arguments, 1, name)?;
    let result = f(as_float_arg(&arguments[0]));
    if result.is_finite() {
        Ok(LpcValue::Float(result))
    } else {
        Ok(LpcValue::Float(0.0))
    }
}

fn sin_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("sin", &arguments, f64::sin)
}

fn cos_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("cos", &arguments, f64::cos)
}

fn tan_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("tan", &arguments, f64::tan)
}

fn asin_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("asin", &arguments, f64::asin)
}

fn acos_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("acos", &arguments, f64::acos)
}

fn atan_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("atan", &arguments, f64::atan)
}

fn log_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("log", &arguments, |v| if v > 0.0 { v.ln() } else { 0.0 })
}

fn exp_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("exp", &arguments, f64::exp)
}

fn floor_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("floor", &arguments, f64::floor)
}

fn ceil_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    float_unary("ceil", &arguments, f64::ceil)
}

fn ctime(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let seconds = arguments
        .first()
        .and_then(LpcValue::as_int)
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    // Simple UTC formatting without chrono dependency.
    Ok(LpcValue::String(format!("ctime({seconds})")))
}

fn allocate(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "allocate")?;
    let size = arguments[0].as_int().unwrap_or(0).max(0) as usize;
    let fill = arguments.get(1).cloned().unwrap_or(LpcValue::Null);
    Ok(LpcValue::Array(vec![fill; size]))
}

fn allocate_mapping(
    _interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::Mapping(IndexMap::new()))
}

fn map_delete(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "map_delete")?;
    let LpcValue::Mapping(mut map) = arguments[0].clone() else {
        bail!("map_delete requires a mapping");
    };
    let key = arguments[1].to_string();
    map.shift_remove(&key);
    Ok(LpcValue::Mapping(map))
}

fn copy_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "copy")?;
    Ok(arguments[0].clone())
}

fn sort_array(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "sort_array")?;
    let LpcValue::Array(mut values) = arguments[0].clone() else {
        bail!("sort_array requires an array");
    };
    if arguments.len() >= 2 {
        let fun = arguments[1].clone();
        let (target, extra): (Option<ObjectRef>, Vec<LpcValue>) = match (&fun, arguments.get(2)) {
            (LpcValue::String(_), Some(LpcValue::Object(object))) => (
                Some(object.clone()),
                arguments.get(3..).unwrap_or(&[]).to_vec(),
            ),
            _ => (None, arguments.get(2..).unwrap_or(&[]).to_vec()),
        };
        let len = values.len();
        for i in 1..len {
            let mut j = i;
            while j > 0 {
                let cmp = compare_sort_pair(
                    interpreter,
                    &fun,
                    &target,
                    &extra,
                    &values[j - 1],
                    &values[j],
                )?;
                if cmp <= 0 {
                    break;
                }
                values.swap(j - 1, j);
                j -= 1;
            }
        }
    } else {
        values.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    }
    Ok(LpcValue::Array(values))
}

fn compare_sort_pair(
    interpreter: &mut Interpreter<'_>,
    fun: &LpcValue,
    target: &Option<ObjectRef>,
    extra: &[LpcValue],
    left: &LpcValue,
    right: &LpcValue,
) -> Result<i64> {
    let mut call_args = vec![left.clone(), right.clone()];
    call_args.extend_from_slice(extra);
    let result = match (fun, target) {
        (LpcValue::String(name), Some(object)) => {
            interpreter.call_function(object.clone(), name, call_args)?
        }
        _ => super::invoke_callable(interpreter, fun, call_args)?,
    };
    Ok(result.as_int().unwrap_or(0))
}

fn base_name(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "base_name")?;
    let path = match &arguments[0] {
        LpcValue::Object(object) => object.lock().name.clone(),
        LpcValue::String(path) => normalize_object_path(path),
        _ => bail!("base_name requires object or string"),
    };
    Ok(LpcValue::String(path))
}

fn regexp_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "regexp")?;
    // Minimal: treat pattern as substring search list API used as regexp(lines, pattern)
    let pattern = arguments[1].to_string();
    match &arguments[0] {
        LpcValue::Array(lines) => {
            let matched: Vec<LpcValue> = lines
                .iter()
                .filter(|line| line.to_string().contains(&pattern))
                .cloned()
                .collect();
            Ok(LpcValue::Array(matched))
        }
        LpcValue::String(text) => Ok(LpcValue::Int(i64::from(text.contains(&pattern)))),
        _ => Ok(LpcValue::Int(0)),
    }
}

fn add_action(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "add_action")?;
    let fun = arguments[0].clone();
    let verb = arguments[1].to_string();
    let flag = arguments
        .get(2)
        .and_then(LpcValue::as_int)
        .unwrap_or(0);
    if !matches!(fun, LpcValue::String(_) | LpcValue::Function(_)) {
        bail!("add_action requires string or function");
    }
    let catch_all = verb.is_empty() || flag != 0;
    // MudOS: sentences live on the command giver (`this_player`), while the
    // defining object (`current_object`) owns the callback function.
    let owner = interpreter.current_object.clone();
    let giver = match &interpreter.this_player {
        // After exec(), login may still be this_player while user::setup()
        // registers cmd_hook/quit on the living — attach to the living.
        Some(tp)
            if owner.lock().commands_enabled && !std::sync::Arc::ptr_eq(tp, &owner) =>
        {
            owner.clone()
        }
        Some(tp) => tp.clone(),
        None => owner.clone(),
    };
    giver.lock().actions.push(Action {
        verb,
        fun,
        catch_all,
        owner,
    });
    Ok(LpcValue::Int(1))
}

fn clear_actions(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    // Drop sentences this object previously registered on the command giver.
    let owner = interpreter.current_object.clone();
    let giver = match &interpreter.this_player {
        Some(tp)
            if owner.lock().commands_enabled && !std::sync::Arc::ptr_eq(tp, &owner) =>
        {
            owner.clone()
        }
        Some(tp) => tp.clone(),
        None => owner.clone(),
    };
    giver
        .lock()
        .actions
        .retain(|action| !std::sync::Arc::ptr_eq(&action.owner, &owner));
    Ok(LpcValue::Int(1))
}

fn notify_fail(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "notify_fail")?;
    let target = interpreter
        .this_player
        .clone()
        .unwrap_or_else(|| interpreter.current_object.clone());
    target.lock().notify_fail = Some(arguments[0].to_string());
    Ok(LpcValue::Int(0))
}

fn query_verb(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let verb = interpreter
        .this_player
        .as_ref()
        .or(Some(&interpreter.current_object))
        .and_then(|object| object.lock().last_verb.clone())
        .unwrap_or_default();
    Ok(LpcValue::String(verb))
}

fn command_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "command")?;
    let line = arguments[0].to_string();
    let player = arguments
        .get(1)
        .and_then(|value| match value {
            LpcValue::Object(object) => Some(object.clone()),
            _ => None,
        })
        .or_else(|| {
            let current = interpreter.current_object.clone();
            if current.lock().interactive.is_some() {
                Some(current)
            } else {
                interpreter.this_player.clone()
            }
        })
        .unwrap_or_else(|| interpreter.current_object.clone());
    let result = interpreter.world.handle_player_command(player, line)?;
    Ok(LpcValue::Int(i64::from(result.is_truthy())))
}

fn commands_efun(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let target = interpreter
        .this_player
        .clone()
        .unwrap_or_else(|| interpreter.current_object.clone());
    let actions = target.lock().actions.clone();
    Ok(LpcValue::Array(
        actions
            .into_iter()
            .map(|action| LpcValue::String(action.verb))
            .collect(),
    ))
}

fn exec_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "exec")?;
    let to = match &arguments[0] {
        LpcValue::Object(object) => object.clone(),
        _ => bail!("exec first argument must be an object"),
    };
    let from = match &arguments[1] {
        LpcValue::Object(object) => object.clone(),
        _ => bail!("exec second argument must be an object"),
    };
    let (interactive, pending) = {
        let mut from_guard = from.lock();
        (from_guard.interactive.take(), from_guard.pending_input.take())
    };
    {
        let mut to_guard = to.lock();
        to_guard.interactive = interactive;
        to_guard.pending_input = pending;
    }
    let _ = interpreter;
    Ok(LpcValue::Int(1))
}

fn crypt_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "crypt")?;
    let password = arguments[0].to_string();
    // MudOS DES crypt uses only the first two salt characters. crypt(pass, 0)
    // (or a missing/null salt) means "choose a new salt".
    let salt = match arguments.get(1) {
        None | Some(LpcValue::Null) | Some(LpcValue::Int(0)) => {
            const ALPHABET: &[u8] =
                b"./0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos() as u64)
                .unwrap_or(0x9e37_79b9_7f4a_7c15);
            let a = ALPHABET[(nanos as usize) % ALPHABET.len()] as char;
            let b = ALPHABET[((nanos >> 8) as usize) % ALPHABET.len()] as char;
            format!("{a}{b}")
        }
        Some(value) => {
            let text = value.to_string();
            let mut chars = text.chars();
            let a = chars.next().unwrap_or('.');
            let b = chars.next().unwrap_or('.');
            format!("{a}{b}")
        }
    };
    let digest = format!("{:x}", simple_hash(&format!("{salt}:{password}")));
    let body = if digest.len() >= 11 {
        digest[..11].to_owned()
    } else {
        format!("{digest:0<11}")
    };
    // Traditional crypt(3) style: 2-char salt + 11-char body (13 total).
    Ok(LpcValue::String(format!("{salt}{body}")))
}

fn simple_hash(input: &str) -> u64 {
    let mut hash = 5381u64;
    for byte in input.bytes() {
        hash = hash.wrapping_mul(33).wrapping_add(u64::from(byte));
    }
    hash
}

fn user_exists(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "user_exists")?;
    let name = arguments[0].as_string().unwrap_or("");
    let path = format!("/adm/save/users/{}/{}.o", &name[..1.min(name.len())], name);
    let resolved = resolve_mudlib_path(interpreter, &path)?;
    Ok(LpcValue::Int(i64::from(resolved.exists())))
}

fn find_player(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "find_player")?;
    let name = arguments[0].as_string().unwrap_or("").to_lowercase();
    for user in interpreter.world.users() {
        let interactive_name = user
            .lock()
            .interactive
            .as_ref()
            .map(|interactive| interactive.name.to_lowercase());
        let living = user.lock().living_name.clone();
        if interactive_name.as_deref() == Some(name.as_str())
            || living.as_deref() == Some(name.as_str())
        {
            return Ok(LpcValue::Object(user));
        }
    }
    Ok(LpcValue::Null)
}

fn find_living(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "find_living")?;
    let name = arguments[0].as_string().unwrap_or("").to_lowercase();
    if let Some(object) = interpreter.world.livings.read().get(&name) {
        if !object.lock().destructed {
            return Ok(LpcValue::Object(object.clone()));
        }
    }
    Ok(LpcValue::Null)
}

fn set_living_name(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "set_living_name")?;
    let name = arguments[0].as_string().unwrap_or("").to_lowercase();
    interpreter.current_object.lock().living_name = Some(name.clone());
    interpreter
        .world
        .livings
        .write()
        .insert(name, interpreter.current_object.clone());
    Ok(LpcValue::Int(1))
}

fn enable_wizard(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    interpreter.current_object.lock().wizard = true;
    Ok(LpcValue::Int(1))
}

fn version_efun(_interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::String("rmudos 0.1".to_owned()))
}

fn mud_name(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::String(interpreter.world.config.mud_name.clone()))
}

fn mudlib_efun(_interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::String("Nightmare".to_owned()))
}

fn mudlib_version(
    _interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::String("3.3 / Darke".to_owned()))
}

fn query_ip_number(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = arguments.first().and_then(|value| match value {
        LpcValue::Object(object) => Some(object.clone()),
        _ => None,
    });
    let Some(object) = object else {
        return Ok(LpcValue::String(String::new()));
    };
    let ip = object
        .lock()
        .interactive
        .as_ref()
        .map(|interactive| interactive.peer.ip().to_string())
        .unwrap_or_default();
    Ok(LpcValue::String(ip))
}

fn query_ip_name(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    query_ip_number(interpreter, arguments)
}

fn save_object(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "save_object")?;
    let path = arguments[0].as_string().context("save_object path")?;
    // MudOS appends ".o" when the path has no extension.
    let path = if path.ends_with(".o") {
        path.to_owned()
    } else {
        format!("{path}.o")
    };
    let resolved = resolve_mudlib_path(interpreter, &path)?;
    if let Some(parent) = resolved.parent() {
        let _ = fs::create_dir_all(parent);
    }
    // Clone globals out before serializing so we never re-lock current_object
    // while holding its mutex (Object values in globals would deadlock).
    let (names, nosave, values) = {
        let object = interpreter.current_object.lock();
        (
            object.program.globals.clone(),
            object.nosave_globals.clone(),
            object.globals.clone(),
        )
    };
    let mut lines = Vec::new();
    for (index, name) in names.iter().enumerate() {
        if nosave.get(index).copied().unwrap_or(false) {
            continue;
        }
        let value = values.get(index).cloned().unwrap_or(LpcValue::Null);
        lines.push(format!("{name} {}\n", serialize_value(&value)));
    }
    let ok = fs::write(&resolved, lines.concat()).is_ok();
    if ok {
        interpreter.world.fs_cache.invalidate(&resolved);
    }
    Ok(LpcValue::Int(i64::from(ok)))
}

fn serialize_value(value: &LpcValue) -> String {
    match value {
        LpcValue::Null => "0".to_owned(),
        LpcValue::Int(v) => v.to_string(),
        LpcValue::Float(v) => v.to_string(),
        LpcValue::String(v) => format!("\"{}\"", v.replace('\\', "\\\\").replace('"', "\\\"")),
        LpcValue::Array(values) => {
            let inner = values
                .iter()
                .map(serialize_value)
                .collect::<Vec<_>>()
                .join(",");
            format!("({{{inner}}})")
        }
        LpcValue::Mapping(map) => {
            let inner = map
                .iter()
                .map(|(k, v)| format!("\"{k}\":{}", serialize_value(v)))
                .collect::<Vec<_>>()
                .join(",");
            format!("([{inner}])")
        }
        LpcValue::Object(object) => {
            // Avoid nested lock if serializer is called while this object is held.
            format!(
                "\"{}\"",
                object
                    .try_lock()
                    .map(|g| g.file_name())
                    .unwrap_or_else(|| "<object>".to_owned())
            )
        }
        LpcValue::Function(_) => "0".to_owned(),
        LpcValue::Class(instance) => {
            let fields = instance.fields.lock();
            let inner = fields.iter().map(serialize_value).collect::<Vec<_>>().join(",");
            format!("(#\"{}\",{})", instance.def.name, inner)
        }
    }
}

fn save_variable(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "save_variable")?;
    Ok(LpcValue::String(serialize_value(&arguments[0])))
}

fn restore_variable(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "restore_variable")?;
    let raw = arguments[0].to_string();
    let classes = interpreter.current_object.lock().program.classes.clone();
    Ok(deserialize_value_with_classes(raw.trim(), &classes))
}

fn restore_object(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "restore_object")?;
    let path = arguments[0].as_string().context("restore_object path")?;
    let path_o = if path.ends_with(".o") {
        path.to_owned()
    } else {
        format!("{path}.o")
    };
    let resolved = resolve_mudlib_path(interpreter, &path_o)
        .or_else(|_| resolve_mudlib_path(interpreter, path))?;
    let Ok(contents) = fs::read_to_string(resolved) else {
        return Ok(LpcValue::Int(0));
    };
    let classes = interpreter.current_object.lock().program.classes.clone();
    let mut object = interpreter.current_object.lock();
    for line in contents.lines() {
        let Some((name, raw)) = line.split_once(' ') else {
            continue;
        };
        if let Some(index) = object.program.globals.iter().position(|g| g == name) {
            object.globals[index] = deserialize_value_with_classes(raw.trim(), &classes);
        }
    }
    Ok(LpcValue::Int(1))
}

fn deserialize_value_with_classes(
    raw: &str,
    classes: &IndexMap<String, Arc<crate::vm::value::ClassDef>>,
) -> LpcValue {
    let raw = raw.trim();
    if raw.is_empty() || raw == "0" {
        return LpcValue::Null;
    }
    if let Ok(v) = raw.parse::<i64>() {
        return LpcValue::Int(v);
    }
    if let Ok(v) = raw.parse::<f64>() {
        if !raw.contains('e') && !raw.contains('E') && !raw.contains('.') {
            // prefer int parse path above; fall through only for real floats
        } else {
            return LpcValue::Float(v);
        }
    }
    if raw.starts_with('"') {
        return parse_quoted_string(raw)
            .map(LpcValue::String)
            .unwrap_or_else(|| LpcValue::String(raw.to_owned()));
    }
    if raw.starts_with("(#") {
        return parse_class_literal(raw, classes).unwrap_or(LpcValue::Null);
    }
    if raw.starts_with("({") && raw.ends_with("})") {
        let inner = &raw[2..raw.len().saturating_sub(2)];
        let parts = split_top_level(inner, ',');
        return LpcValue::Array(
            parts
                .into_iter()
                .map(|part| deserialize_value_with_classes(part.trim(), classes))
                .collect(),
        );
    }
    if raw.starts_with("([") && raw.ends_with("])") {
        let inner = &raw[2..raw.len().saturating_sub(2)];
        let parts = split_top_level(inner, ',');
        let mut map = IndexMap::new();
        for part in parts {
            let part = part.trim();
            if part.is_empty() {
                continue;
            }
            if let Some((key_raw, value_raw)) = split_once_top_level(part, ':') {
                let key = parse_quoted_string(key_raw.trim())
                    .unwrap_or_else(|| key_raw.trim().to_owned());
                map.insert(
                    key,
                    deserialize_value_with_classes(value_raw.trim(), classes),
                );
            }
        }
        return LpcValue::Mapping(map);
    }
    LpcValue::String(raw.to_owned())
}

fn parse_class_literal(
    raw: &str,
    classes: &IndexMap<String, Arc<crate::vm::value::ClassDef>>,
) -> Option<LpcValue> {
    // (#"name",v0,v1,...)
    let body = raw.strip_prefix("(#")?.strip_suffix(')')?;
    let parts = split_top_level(body, ',');
    let mut parts = parts.into_iter();
    let name_raw = parts.next()?.trim();
    let name = parse_quoted_string(name_raw).unwrap_or_else(|| name_raw.to_owned());
    let values: Vec<LpcValue> = parts
        .map(|part| deserialize_value_with_classes(part.trim(), classes))
        .collect();
    let def = classes.get(&name).cloned().unwrap_or_else(|| {
        Arc::new(crate::vm::value::ClassDef {
            name: name.clone(),
            fields: (0..values.len()).map(|i| format!("field{i}")).collect(),
        })
    });
    let instance = crate::vm::value::ClassInstance::new(def);
    {
        let mut fields = instance.fields.lock();
        for (index, value) in values.into_iter().enumerate() {
            if index < fields.len() {
                fields[index] = value;
            }
        }
    }
    Some(LpcValue::Class(instance))
}

fn parse_quoted_string(raw: &str) -> Option<String> {
    if !(raw.starts_with('"') && raw.ends_with('"') && raw.len() >= 2) {
        return None;
    }
    Some(raw[1..raw.len() - 1].replace("\\\"", "\"").replace("\\\\", "\\"))
}

fn split_top_level(input: &str, delimiter: char) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0i32;
    let mut in_string = false;
    let mut escaped = false;
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    for (idx, ch) in chars.iter().copied() {
        if in_string {
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }
        match ch {
            '"' => in_string = true,
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            c if c == delimiter && depth == 0 => {
                parts.push(&input[start..idx]);
                start = idx + ch.len_utf8();
            }
            _ => {}
        }
    }
    if start <= input.len() {
        parts.push(&input[start..]);
    }
    parts
}

fn split_once_top_level<'a>(input: &'a str, delimiter: char) -> Option<(&'a str, &'a str)> {
    let parts = split_top_level(input, delimiter);
    if parts.len() < 2 {
        return None;
    }
    let key = parts[0];
    let rest_start = key.len() + delimiter.len_utf8();
    Some((key, &input[rest_start..]))
}

fn shadow_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "shadow")?;
    let target = match &arguments[0] {
        LpcValue::Object(object) => object.clone(),
        _ => return Ok(LpcValue::Null),
    };
    let flag = arguments.get(1).and_then(LpcValue::as_int).unwrap_or(1);
    if flag == 0 {
        // Query: return the object currently shadowing `target`, or 0.
        let shadow = target.lock().shadow.clone();
        return Ok(shadow.map(LpcValue::Object).unwrap_or(LpcValue::Null));
    }
    let shadow = interpreter.current_object.clone();
    target.lock().shadow = Some(shadow.clone());
    shadow.lock().shadowed = Some(Arc::downgrade(&target));
    Ok(LpcValue::Object(shadow))
}

fn query_shadowing(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "query_shadowing")?;
    let object = match &arguments[0] {
        LpcValue::Object(object) => object.clone(),
        _ => return Ok(LpcValue::Null),
    };
    let shadowed = object
        .lock()
        .shadowed
        .as_ref()
        .and_then(std::sync::Weak::upgrade);
    Ok(shadowed.map(LpcValue::Object).unwrap_or(LpcValue::Null))
}

fn next_shadow(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let object = arguments
        .first()
        .and_then(|value| match value {
            LpcValue::Object(object) => Some(object.clone()),
            _ => None,
        })
        .ok_or_else(|| anyhow::anyhow!("next_shadow requires an object"))?;
    let shadow = object.lock().shadow.clone();
    Ok(shadow.map(LpcValue::Object).unwrap_or(LpcValue::Null))
}

fn bind_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "bind")?;
    let fun = arguments[0].clone();
    let owner = match &arguments[1] {
        LpcValue::Object(object) => object.clone(),
        _ => bail!("bind requires an object"),
    };
    match fun {
        LpcValue::Function(function) => {
            let mut cloned = (*function).clone();
            cloned.owner = owner;
            Ok(LpcValue::Function(Arc::new(cloned)))
        }
        LpcValue::String(name) => Ok(LpcValue::Function(Arc::new(
            crate::vm::value::LpcFunction {
                owner,
                kind: crate::vm::value::FunctionKind::Named {
                    name,
                    bound: Vec::new(),
                },
            },
        ))),
        other => Ok(other),
    }
}

fn map_alias(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    if arguments
        .first()
        .is_some_and(|value| matches!(value, LpcValue::Mapping(_)))
    {
        map_mapping(interpreter, arguments)
    } else {
        super::map_array(interpreter, arguments)
    }
}

fn map_mapping(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "map_mapping")?;
    let LpcValue::Mapping(mapping) = &arguments[0] else {
        bail!("map_mapping first argument must be a mapping");
    };
    let fun = &arguments[1];
    let (target, extra): (Option<ObjectRef>, &[LpcValue]) = match (fun, arguments.get(2)) {
        (LpcValue::String(_), Some(LpcValue::Object(object))) => {
            (Some(object.clone()), arguments.get(3..).unwrap_or(&[]))
        }
        _ => (None, arguments.get(2..).unwrap_or(&[])),
    };
    let mut result = IndexMap::new();
    for (key, value) in mapping {
        let mut call_args = vec![LpcValue::String(key.clone()), value.clone()];
        call_args.extend_from_slice(extra);
        let mapped = match (fun, &target) {
            (LpcValue::String(name), Some(object)) => {
                interpreter.call_function(object.clone(), name, call_args)
            }
            _ => super::invoke_callable(interpreter, fun, call_args),
        }
        .context("map_mapping callback failed")?;
        result.insert(key.clone(), mapped);
    }
    Ok(LpcValue::Mapping(result))
}

fn set_hide(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "set_hide")?;
    let hide = arguments[0].is_truthy();
    let target = interpreter.current_object.clone();
    if hide {
        let Some(master) = interpreter.world.master() else {
            return Ok(LpcValue::Int(0));
        };
        let allowed = interpreter.world.apply(
            master,
            "valid_hide",
            vec![LpcValue::Object(target.clone())],
            interpreter.this_player.clone(),
            Some(interpreter.current_object.clone()),
        )?;
        if !allowed.is_truthy() {
            return Ok(LpcValue::Int(0));
        }
        target.lock().can_hide = true;
    }
    target.lock().hidden = hide;
    Ok(LpcValue::Int(1))
}

fn unlink_snooper(snooper: &ObjectRef) {
    let target = snooper.lock().snoop_target.take();
    if let Some(target) = target {
        let mut target_guard = target.lock();
        if target_guard
            .snooper
            .as_ref()
            .is_some_and(|observer| std::sync::Arc::ptr_eq(observer, snooper))
        {
            target_guard.snooper = None;
        }
    }
}

fn link_snoop(snooper: &ObjectRef, snoopee: &ObjectRef) {
    unlink_snooper(snooper);
    {
        let mut snoopee_guard = snoopee.lock();
        if let Some(previous) = snoopee_guard.snooper.take() {
            if !std::sync::Arc::ptr_eq(&previous, snooper) {
                previous.lock().snoop_target = None;
            }
        }
        snoopee_guard.snooper = Some(snooper.clone());
    }
    snooper.lock().snoop_target = Some(snoopee.clone());
}

fn snoop_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "snoop")?;
    let snooper = super::object_argument(&arguments[0], "snoop")?;
    if arguments.len() == 1 {
        return Ok(snooper
            .lock()
            .snoop_target
            .clone()
            .map(LpcValue::Object)
            .unwrap_or(LpcValue::Null));
    }
    let snoopee = match &arguments[1] {
        LpcValue::Null | LpcValue::Int(0) => {
            unlink_snooper(&snooper);
            return Ok(LpcValue::Int(1));
        }
        value => super::object_argument(value, "snoop")?,
    };
    link_snoop(&snooper, &snoopee);
    Ok(LpcValue::Object(snoopee))
}

fn query_snoop(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let snoopee = if let Some(value) = arguments.first() {
        super::object_argument(value, "query_snoop")?
    } else {
        bail!("query_snoop requires an object");
    };
    let result = snoopee
        .lock()
        .snooper
        .clone()
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null);
    Ok(result)
}

fn query_snooping(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let observer = if let Some(value) = arguments.first() {
        super::object_argument(value, "query_snooping")?
    } else {
        bail!("query_snooping requires an object");
    };
    let result = observer
        .lock()
        .snoop_target
        .clone()
        .map(LpcValue::Object)
        .unwrap_or(LpcValue::Null);
    Ok(result)
}

fn program_inherits(program: &Program, path: &str) -> bool {
    let path = normalize_object_path(path);
    program.inherit_programs.iter().any(|inherit| {
        inherit.path == path || program_inherits(inherit, &path)
    })
}

fn inherits_efun(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "inherits")?;
    let path = arguments[0]
        .as_string()
        .context("inherits requires a file path")?;
    let object = super::object_argument(&arguments[1], "inherits")?;
    let program = object.lock().program.clone();
    Ok(LpcValue::Int(i64::from(program_inherits(&program, path))))
}

fn collect_deep_inherits(program: &Program, out: &mut Vec<String>) {
    for inherit in &program.inherit_programs {
        out.push(inherit.path.clone());
        collect_deep_inherits(inherit, out);
    }
}

fn deep_inherit_list(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        super::object_argument(value, "deep_inherit_list")?
    } else {
        interpreter.current_object.clone()
    };
    let program = object.lock().program.clone();
    let mut paths = Vec::new();
    collect_deep_inherits(&program, &mut paths);
    Ok(LpcValue::Array(
        paths.into_iter().map(LpcValue::String).collect(),
    ))
}

fn call_out_info(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let entries = interpreter.world.call_outs.info();
    Ok(LpcValue::Array(
        entries
            .into_iter()
            .map(|(object, fun, delay, args)| {
                let mut row = vec![
                    LpcValue::Object(object),
                    fun,
                    LpcValue::Int(delay),
                ];
                if args.is_empty() {
                    LpcValue::Array(row)
                } else if args.len() == 1 {
                    row.push(args.into_iter().next().unwrap());
                    LpcValue::Array(row)
                } else {
                    row.push(LpcValue::Array(args));
                    LpcValue::Array(row)
                }
            })
            .collect(),
    ))
}

fn stat_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "stat")?;
    let path = arguments[0]
        .as_string()
        .context("stat requires a path")?
        .to_string();
    let mode = arguments.get(1).and_then(LpcValue::as_int).unwrap_or(0);
    if mode == -1 {
        return get_dir(
            interpreter,
            vec![LpcValue::String(path.clone()), LpcValue::Int(-1)],
        );
    }
    let resolved = resolve_mudlib_path(interpreter, &path)?;
    let stat = interpreter.world.fs_cache.stat(&resolved);
    if stat.is_dir() {
        return get_dir(interpreter, vec![LpcValue::String(path)]);
    }
    if !stat.exists() {
        return Ok(LpcValue::Int(-1));
    }
    Ok(LpcValue::Array(vec![
        LpcValue::Int(stat.file_size()),
        LpcValue::Int(stat.mtime()),
        LpcValue::Int(0),
    ]))
}

fn ed_efun(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "ed")?;
    let _file = arguments[0]
        .as_string()
        .context("ed requires a file name")?;
    // Full MudOS ed is not implemented; return 0 so mudlib simple editor can run.
    interpreter.current_object.lock().editing_file = None;
    Ok(LpcValue::Int(0))
}

fn in_edit(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        super::object_argument(value, "in_edit")?
    } else {
        interpreter.current_object.clone()
    };
    let editing = {
        let guard = object.lock();
        guard
            .editing_file
            .as_deref()
            .is_some_and(|name| !name.is_empty())
    };
    Ok(LpcValue::Int(i64::from(editing)))
}

fn in_input(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    let object = if let Some(value) = arguments.first() {
        super::object_argument(value, "in_input")?
    } else {
        interpreter.current_object.clone()
    };
    let pending = object.lock().pending_input.is_some();
    Ok(LpcValue::Int(i64::from(pending)))
}

fn filter_alias(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    if arguments
        .first()
        .is_some_and(|value| matches!(value, LpcValue::Mapping(_)))
    {
        filter_mapping(interpreter, arguments)
    } else {
        super::filter_array(interpreter, arguments)
    }
}

fn filter_mapping(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "filter_mapping")?;
    let LpcValue::Mapping(mapping) = &arguments[0] else {
        bail!("filter_mapping first argument must be a mapping");
    };
    let fun = &arguments[1];
    let (target, extra): (Option<ObjectRef>, &[LpcValue]) = match (fun, arguments.get(2)) {
        (LpcValue::String(_), Some(LpcValue::Object(object))) => {
            (Some(object.clone()), arguments.get(3..).unwrap_or(&[]))
        }
        _ => (None, arguments.get(2..).unwrap_or(&[])),
    };
    let mut result = IndexMap::new();
    for (key, value) in mapping {
        let mut call_args = vec![LpcValue::String(key.clone()), value.clone()];
        call_args.extend_from_slice(extra);
        let keep = match (fun, &target) {
            (LpcValue::String(name), Some(object)) => {
                interpreter.call_function(object.clone(), name, call_args)
            }
            _ => super::invoke_callable(interpreter, fun, call_args),
        }
        .context("filter_mapping callback failed")?;
        if keep.is_truthy() {
            result.insert(key.clone(), value.clone());
        }
    }
    Ok(LpcValue::Mapping(result))
}

fn unique_array(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 2, "unique_array")?;
    let LpcValue::Array(values) = &arguments[0] else {
        bail!("unique_array first argument must be an array");
    };
    let fun = &arguments[1];
    let skip = arguments.get(2);
    let mut groups: IndexMap<String, Vec<LpcValue>> = IndexMap::new();
    for item in values {
        if skip.is_some_and(|skip| item == skip) {
            continue;
        }
        let key = match (fun, item) {
            (LpcValue::String(name), LpcValue::Object(object)) => {
                interpreter.call_function(object.clone(), name, Vec::new())?
            }
            _ => super::invoke_callable(interpreter, fun, vec![item.clone()])?
        };
        groups
            .entry(unique_group_key(&key))
            .or_default()
            .push(item.clone());
    }
    Ok(LpcValue::Array(
        groups
            .into_values()
            .map(LpcValue::Array)
            .collect(),
    ))
}

fn unique_group_key(value: &LpcValue) -> String {
    match value {
        LpcValue::Object(object) => format!("o:{}", object.lock().id),
        other => format!("{}:{}", other.type_name(), other),
    }
}

fn children_efun(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "children")?;
    let path = arguments[0]
        .as_string()
        .context("children requires a file path")?;
    let path = normalize_object_path(path);
    let path = path.strip_suffix(".c").unwrap_or(path.as_str()).to_owned();
    Ok(LpcValue::Array(
        interpreter
            .world
            .all_objects()
            .into_iter()
            .filter(|object| object.lock().name == path)
            .map(LpcValue::Object)
            .collect(),
    ))
}

fn objects_efun(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let objects: Vec<LpcValue> = interpreter
        .world
        .all_objects()
        .into_iter()
        .map(LpcValue::Object)
        .collect();
    if arguments.is_empty() {
        return Ok(LpcValue::Array(objects));
    }
    let mut filter_args = vec![LpcValue::Array(objects), arguments[0].clone()];
    filter_args.extend_from_slice(&arguments[1..]);
    super::filter_array(interpreter, filter_args)
}

fn livings_efun(
    interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::Array(
        interpreter
            .world
            .all_objects()
            .into_iter()
            .filter(|object| {
                let guard = object.lock();
                guard.commands_enabled || guard.living_name.is_some()
            })
            .map(LpcValue::Object)
            .collect(),
    ))
}

fn uptime_efun(interpreter: &mut Interpreter<'_>, _arguments: Vec<LpcValue>) -> Result<LpcValue> {
    Ok(LpcValue::Int(
        interpreter.world.started.elapsed().as_secs() as i64,
    ))
}

fn localtime_efun(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let stamp = arguments
        .first()
        .and_then(LpcValue::as_int)
        .unwrap_or_else(|| {
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0)
        });
    Ok(LpcValue::Array(unix_localtime_utc(stamp)))
}

/// MudOS `localtime` array in UTC: sec, min, hour, mday, mon(0-11), year, wday, yday, gmtoff, zone.
fn unix_localtime_utc(stamp: i64) -> Vec<LpcValue> {
    let stamp = stamp.max(0);
    let sec = stamp % 60;
    let minutes = stamp / 60;
    let min = minutes % 60;
    let hours = minutes / 60;
    let hour = hours % 24;
    let days = hours / 24;
    let wday = ((days + 4) % 7) as i64; // 1970-01-01 was Thursday
    let (year, yday, month, mday) = civil_from_unix_days(days);
    vec![
        LpcValue::Int(sec),
        LpcValue::Int(min),
        LpcValue::Int(hour),
        LpcValue::Int(mday),
        LpcValue::Int(month),
        LpcValue::Int(year),
        LpcValue::Int(wday),
        LpcValue::Int(yday),
        LpcValue::Int(0),
        LpcValue::String("UTC".to_owned()),
    ]
}

fn civil_from_unix_days(days: i64) -> (i64, i64, i64, i64) {
    let mut year = 1970;
    let mut remaining = days;
    loop {
        let length = if is_leap_year(year) { 366 } else { 365 };
        if remaining < length {
            break;
        }
        remaining -= length;
        year += 1;
    }
    let yday = remaining;
    const MONTHS: [i64; 12] = [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 0;
    let mut day = remaining;
    for (index, &len) in MONTHS.iter().enumerate() {
        let len = if index == 1 && is_leap_year(year) {
            29
        } else {
            len
        };
        if day < len {
            month = index as i64;
            break;
        }
        day -= len;
        month = index as i64;
    }
    (year, yday, month, day + 1)
}

fn is_leap_year(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

fn remove_action(interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "remove_action")?;
    let first = arguments[0].to_string();
    let second = arguments.get(1).map(ToString::to_string);
    let owner = interpreter.current_object.clone();
    let giver = interpreter
        .this_player
        .clone()
        .unwrap_or_else(|| owner.clone());
    let mut guard = giver.lock();
    let before = guard.actions.len();
    guard.actions.retain(|action| {
        if !std::sync::Arc::ptr_eq(&action.owner, &owner) {
            return true;
        }
        match &second {
            Some(verb) => {
                let fun_name = match &action.fun {
                    LpcValue::String(name) => name.as_str(),
                    _ => "",
                };
                !(fun_name == first && action.verb == *verb)
            }
            None => action.verb != first,
        }
    });
    let removed = before != guard.actions.len();
    Ok(LpcValue::Int(i64::from(removed)))
}

fn virtualp(_interpreter: &mut Interpreter<'_>, arguments: Vec<LpcValue>) -> Result<LpcValue> {
    require(&arguments, 1, "virtualp")?;
    // Virtual compile_object objects are not distinguished yet.
    let _ = arguments;
    Ok(LpcValue::Int(0))
}

fn parse_command(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    let values = parse_command_match(interpreter, &arguments)?;
    Ok(LpcValue::Int(i64::from(values.is_some())))
}

fn parse_command_values(
    interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    match parse_command_match(interpreter, &arguments)? {
        Some(captures) => {
            let mut row = vec![LpcValue::Int(1)];
            row.extend(captures);
            Ok(LpcValue::Array(row))
        }
        None => Ok(LpcValue::Array(vec![LpcValue::Int(0)])),
    }
}

fn parse_command_match(
    interpreter: &mut Interpreter<'_>,
    arguments: &[LpcValue],
) -> Result<Option<Vec<LpcValue>>> {
    require(arguments, 3, "parse_command")?;
    let command = arguments[0].to_string();
    let pattern = arguments[2].to_string();
    let objects = parse_command_objects(interpreter, &arguments[1])?;
    let tokens = tokenize_parse_pattern(&pattern);
    let words: Vec<String> = command.split_whitespace().map(str::to_owned).collect();
    Ok(match_parse_tokens(&words, &tokens, &objects))
}

fn parse_command_objects(
    _interpreter: &mut Interpreter<'_>,
    env: &LpcValue,
) -> Result<Vec<ObjectRef>> {
    match env {
        LpcValue::Object(object) => {
            let mut list = vec![object.clone()];
            let mut stack = object.lock().inventory.clone();
            while let Some(child) = stack.pop() {
                if child.lock().destructed {
                    continue;
                }
                let nested = child.lock().inventory.clone();
                list.push(child);
                stack.extend(nested);
            }
            Ok(list)
        }
        LpcValue::Array(values) => Ok(values
            .iter()
            .filter_map(|value| match value {
                LpcValue::Object(object) if !object.lock().destructed => Some(object.clone()),
                _ => None,
            })
            .collect()),
        _ => Ok(Vec::new()),
    }
}

#[derive(Clone, Debug)]
enum ParseTok {
    Word { text: String, optional: bool },
    Alt,
    Spec(char),
}

fn tokenize_parse_pattern(pattern: &str) -> Vec<ParseTok> {
    let mut tokens = Vec::new();
    let chars: Vec<char> = pattern.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            ' ' | '\t' | '\n' => i += 1,
            '/' => {
                tokens.push(ParseTok::Alt);
                i += 1;
            }
            '%' if i + 1 < chars.len() => {
                tokens.push(ParseTok::Spec(chars[i + 1]));
                i += 2;
            }
            '\'' | '"' => {
                let quote = chars[i];
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != quote {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(ParseTok::Word {
                    text,
                    optional: false,
                });
            }
            '[' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i] != ']' {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                if i < chars.len() {
                    i += 1;
                }
                tokens.push(ParseTok::Word {
                    text: text.trim().trim_matches('\'').trim_matches('"').to_owned(),
                    optional: true,
                });
            }
            _ => {
                let start = i;
                while i < chars.len() && !matches!(chars[i], ' ' | '\t' | '\n' | '/' | '%') {
                    i += 1;
                }
                let text: String = chars[start..i].iter().collect();
                if !text.is_empty() {
                    tokens.push(ParseTok::Word {
                        text,
                        optional: false,
                    });
                }
            }
        }
    }
    tokens
}

fn match_parse_tokens(
    words: &[String],
    tokens: &[ParseTok],
    objects: &[ObjectRef],
) -> Option<Vec<LpcValue>> {
    match_parse_tokens_at(words, 0, tokens, 0, objects)
}

fn match_parse_tokens_at(
    words: &[String],
    wi: usize,
    tokens: &[ParseTok],
    ti: usize,
    objects: &[ObjectRef],
) -> Option<Vec<LpcValue>> {
    if ti >= tokens.len() {
        return if wi >= words.len() {
            Some(Vec::new())
        } else {
            None
        };
    }
    let (alts, next_ti) = collect_alternatives(tokens, ti);
    if alts.iter().any(|tok| matches!(tok, ParseTok::Spec(_))) {
        let spec = alts.iter().find_map(|tok| match tok {
            ParseTok::Spec(ch) => Some(*ch),
            _ => None,
        })?;
        return match_spec(spec, words, wi, tokens, next_ti, objects);
    }
    let optional = alts.iter().any(|tok| matches!(
        tok,
        ParseTok::Word { optional: true, .. }
    ));
    let options: Vec<String> = alts
        .iter()
        .filter_map(|tok| match tok {
            ParseTok::Word { text, .. } => Some(text.to_ascii_lowercase()),
            _ => None,
        })
        .collect();
    if wi < words.len() && options.iter().any(|opt| opt == &words[wi].to_ascii_lowercase())
    {
        return match_parse_tokens_at(words, wi + 1, tokens, next_ti, objects);
    }
    if optional {
        return match_parse_tokens_at(words, wi, tokens, next_ti, objects);
    }
    None
}

fn collect_alternatives(tokens: &[ParseTok], start: usize) -> (Vec<ParseTok>, usize) {
    let mut alts = Vec::new();
    let mut i = start;
    let mut expect_token = true;
    while i < tokens.len() {
        match &tokens[i] {
            ParseTok::Alt if !expect_token => {
                expect_token = true;
                i += 1;
            }
            ParseTok::Alt => i += 1,
            tok if expect_token => {
                alts.push(tok.clone());
                expect_token = false;
                i += 1;
            }
            _ => break,
        }
    }
    (alts, i)
}

fn match_spec(
    spec: char,
    words: &[String],
    wi: usize,
    tokens: &[ParseTok],
    next_ti: usize,
    objects: &[ObjectRef],
) -> Option<Vec<LpcValue>> {
    match spec {
        's' => {
            if next_ti >= tokens.len() {
                let rest = words[wi..].join(" ");
                return Some(vec![LpcValue::String(rest)]);
            }
            for end in wi..=words.len() {
                if let Some(mut rest) =
                    match_parse_tokens_at(words, end, tokens, next_ti, objects)
                {
                    rest.insert(0, LpcValue::String(words[wi..end].join(" ")));
                    return Some(rest);
                }
            }
            None
        }
        'w' => {
            let word = words.get(wi)?.clone();
            let mut rest = match_parse_tokens_at(words, wi + 1, tokens, next_ti, objects)?;
            rest.insert(0, LpcValue::String(word));
            Some(rest)
        }
        'd' => {
            let word = words.get(wi)?;
            let number = word.parse::<i64>().ok()?;
            let mut rest = match_parse_tokens_at(words, wi + 1, tokens, next_ti, objects)?;
            rest.insert(0, LpcValue::Int(number));
            Some(rest)
        }
        'o' | 'i' | 'l' => {
            let word = words.get(wi)?;
            let matched: Vec<ObjectRef> = objects
                .iter()
                .filter(|object| object_matches_parse(object, word, spec == 'l'))
                .cloned()
                .collect();
            if matched.is_empty() {
                return None;
            }
            let mut rest = match_parse_tokens_at(words, wi + 1, tokens, next_ti, objects)?;
            let value = if spec == 'o' {
                LpcValue::Object(matched[0].clone())
            } else {
                let mut row = vec![LpcValue::Int(1)];
                row.extend(matched.into_iter().map(LpcValue::Object));
                LpcValue::Array(row)
            };
            rest.insert(0, value);
            Some(rest)
        }
        'p' => {
            let word = words.get(wi)?.clone();
            let mut rest = match_parse_tokens_at(words, wi + 1, tokens, next_ti, objects)?;
            rest.insert(0, LpcValue::String(word));
            Some(rest)
        }
        _ => None,
    }
}

fn object_matches_parse(object: &ObjectRef, word: &str, living_only: bool) -> bool {
    let guard = object.lock();
    if guard.destructed {
        return false;
    }
    if living_only && guard.living_name.is_none() && !guard.commands_enabled {
        return false;
    }
    if guard
        .living_name
        .as_deref()
        .is_some_and(|name| name.eq_ignore_ascii_case(word))
    {
        return true;
    }
    let file = guard.file_name();
    drop(guard);
    file.eq_ignore_ascii_case(word) || file.ends_with(word)
}

/// MudOS `EESOCKET` — problem creating socket / sockets unsupported.
const EESOCKET: i64 = -1;
/// MudOS `EEMODENOTSUPP`.
const EEMODENOTSUPP: i64 = -12;

fn socket_create(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 2, "socket_create")?;
    // Not implemented: network daemons expect a negative error code.
    Ok(LpcValue::Int(EESOCKET))
}

fn socket_unsupported(
    _interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::Int(EEMODENOTSUPP))
}

fn socket_close_stub(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "socket_close")?;
    // Closing a non-existent fd is treated as success in this stub.
    Ok(LpcValue::Int(1))
}

fn socket_address_stub(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "socket_address")?;
    Ok(LpcValue::String(String::new()))
}

fn socket_error_stub(
    _interpreter: &mut Interpreter<'_>,
    arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    require(&arguments, 1, "socket_error")?;
    let code = arguments[0].as_int().unwrap_or(0);
    let msg = match code {
        EESOCKET => "Problem creating socket",
        EEMODENOTSUPP => "Socket mode not supported",
        1 => "Operation successful",
        _ => "Socket operation failed",
    };
    Ok(LpcValue::String(msg.to_owned()))
}

fn dump_socket_status(
    _interpreter: &mut Interpreter<'_>,
    _arguments: Vec<LpcValue>,
) -> Result<LpcValue> {
    Ok(LpcValue::Int(1))
}

#[allow(dead_code)]
fn _use_object_ref(_: ObjectRef) {}
