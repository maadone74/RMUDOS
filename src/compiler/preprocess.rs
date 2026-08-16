//! Minimal MudOS-style preprocessor: `#include`, `#define`, `#undef`, `#ifdef`/`#ifndef`/`#if`/`#else`/`#endif`.

use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

pub fn preprocess(source: &str, file_path: &Path, mudlib_root: &Path) -> Result<String> {
    let mut state = Preprocessor {
        macros: HashMap::new(),
        including: HashSet::new(),
        mudlib_root: mudlib_root.to_path_buf(),
        include_dirs: vec![mudlib_root.join("adm").join("include")],
    };
    // Driver-provided defines that MudOS normally injects via config.h / command line.
    state.macros.insert("MUD_NAME".to_owned(), "\"RustMud\"".to_owned());
    state.macros.insert("__PORT__".to_owned(), "4000".to_owned());
    state.macros.insert("__VERSION__".to_owned(), "\"rmudos 0.1\"".to_owned());
    state.macros.insert("MUDOS_VERSION".to_owned(), "\"rmudos 0.1\"".to_owned());
    // MudOS-style global include (defines TO, TP, …).
    let mut preamble = String::new();
    for name in ["globals.h", "global.h"] {
        let candidate = mudlib_root.join("adm").join("include").join(name);
        if candidate.is_file() {
            let global = fs::read_to_string(&candidate)
                .with_context(|| format!("failed to read {}", candidate.display()))?;
            preamble.push_str(
                &state
                    .preprocess_file(&global, &candidate)
                    .with_context(|| format!("while preprocessing {}", candidate.display()))?,
            );
            break;
        }
    }
    let body = state.preprocess_file(source, file_path)?;
    Ok(preamble + &body)
}

struct Preprocessor {
    macros: HashMap<String, String>,
    including: HashSet<PathBuf>,
    mudlib_root: PathBuf,
    include_dirs: Vec<PathBuf>,
}

impl Preprocessor {
    fn preprocess_file(&mut self, source: &str, file_path: &Path) -> Result<String> {
        let canonical = file_path.to_path_buf();
        if !self.including.insert(canonical.clone()) {
            bail!("recursive #include of {}", file_path.display());
        }
        let result = self.preprocess_source(source, file_path);
        self.including.remove(&canonical);
        result
    }

    fn preprocess_source(&mut self, source: &str, file_path: &Path) -> Result<String> {
        let source = join_line_continuations(source);
        let mut output = String::new();
        let mut false_stack: Vec<bool> = Vec::new();
        let mut skip_depth = 0usize;

        for line in source.lines() {
            let trimmed = trim_directive_prefix(line);
            if let Some(rest) = trimmed.strip_prefix('#') {
                let rest = rest.trim_start();
                let (directive, args) = split_directive(rest);
                match directive {
                    "include" if skip_depth == 0 => {
                        let included = self
                            .load_include(args, file_path)
                            .with_context(|| format!("in {}", file_path.display()))?;
                        output.push_str(&included);
                        if !included.ends_with('\n') {
                            output.push('\n');
                        }
                    }
                    "define" if skip_depth == 0 => {
                        let (name, value) = parse_define(args)?;
                        self.macros.insert(name, value);
                    }
                    "undef" if skip_depth == 0 => {
                        let name = args.split_whitespace().next().unwrap_or("");
                        self.macros.remove(name);
                    }
                    "ifdef" => {
                        let name = args.split_whitespace().next().unwrap_or("");
                        let take = self.macros.contains_key(name);
                        false_stack.push(!take);
                        if !take {
                            skip_depth += 1;
                        }
                    }
                    "ifndef" => {
                        let name = args.split_whitespace().next().unwrap_or("");
                        let take = !self.macros.contains_key(name);
                        false_stack.push(!take);
                        if !take {
                            skip_depth += 1;
                        }
                    }
                    "if" => {
                        // Minimal: `#if 0` / `#if 1` only.
                        let take = args.trim() != "0" && !args.trim().is_empty();
                        false_stack.push(!take);
                        if !take {
                            skip_depth += 1;
                        }
                    }
                    "else" => {
                        let Some(was_false) = false_stack.last_mut() else {
                            bail!("#else without #if in {}", file_path.display());
                        };
                        if *was_false {
                            *was_false = false;
                            skip_depth = skip_depth.saturating_sub(1);
                        } else {
                            *was_false = true;
                            skip_depth += 1;
                        }
                    }
                    "endif" => {
                        let Some(was_false) = false_stack.pop() else {
                            bail!("#endif without #if in {}", file_path.display());
                        };
                        if was_false {
                            skip_depth = skip_depth.saturating_sub(1);
                        }
                    }
                    "pragma" | "error" | "warning" | "echo" => {}
                    _ if skip_depth > 0 => {}
                    _ => {}
                }
                continue;
            }

            if skip_depth > 0 {
                continue;
            }
            output.push_str(&expand_macros(line, &self.macros));
            output.push('\n');
        }
        Ok(output)
    }

    fn load_include(&mut self, args: &str, from_file: &Path) -> Result<String> {
        let args = args.trim();
        let (path, system) = if let Some(rest) = args.strip_prefix('<') {
            // Allow trailing comments: `#include <ansi.h>  // note`
            let end = rest.find('>').with_context(|| format!("malformed #include {args}"))?;
            let name = rest[..end].trim();
            (name.to_owned(), true)
        } else if let Some(rest) = args.strip_prefix('"') {
            let end = rest.find('"').with_context(|| format!("malformed #include {args}"))?;
            let name = rest[..end].trim();
            (name.to_owned(), false)
        } else {
            bail!("malformed #include {args}");
        };

        let resolved = if system || path.starts_with('/') {
            self.resolve_system_include(&path)?
        } else {
            from_file
                .parent()
                .unwrap_or_else(|| Path::new("."))
                .join(&path)
        };

        let source = fs::read_to_string(&resolved)
            .with_context(|| format!("failed to read include {}", resolved.display()))?;
        self.preprocess_file(&source, &resolved)
    }

    fn resolve_system_include(&self, path: &str) -> Result<PathBuf> {
        let relative = path.trim_start_matches('/');
        for dir in &self.include_dirs {
            let candidate = dir.join(relative);
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
        let candidate = self.mudlib_root.join(relative);
        if candidate.is_file() {
            return Ok(candidate);
        }
        bail!("include not found: {path}")
    }
}

fn join_line_continuations(source: &str) -> String {
    let mut output = String::with_capacity(source.len());
    let lines: Vec<&str> = source.lines().collect();
    let mut i = 0;
    while i < lines.len() {
        let mut line = lines[i].to_owned();
        while line.trim_end().ends_with('\\') {
            let trimmed = line.trim_end();
            line = trimmed[..trimmed.len() - 1].to_owned();
            i += 1;
            if i >= lines.len() {
                break;
            }
            line.push_str(lines[i]);
        }
        output.push_str(&line);
        output.push('\n');
        i += 1;
    }
    output
}

fn trim_directive_prefix(line: &str) -> &str {
    let mut chars = line.chars();
    while let Some(ch) = chars.next() {
        if ch == ' ' || ch == '\t' {
            continue;
        }
        if ch == '#' {
            return line.trim_start();
        }
        break;
    }
    line
}

fn split_directive(rest: &str) -> (&str, &str) {
    let rest = rest.trim_start();
    let end = rest
        .find(|ch: char| ch.is_whitespace())
        .unwrap_or(rest.len());
    let directive = &rest[..end];
    let args = rest[end..].trim_start();
    (directive, args)
}

fn parse_define(args: &str) -> Result<(String, String)> {
    let args = args.trim();
    if args.is_empty() {
        bail!("#define missing name");
    }
    // Function-like macros: NAME(args) body — store whole body after name for now as empty/not supported specially
    let name_end = args
        .find(|ch: char| ch.is_whitespace() || ch == '(')
        .unwrap_or(args.len());
    let name = args[..name_end].to_owned();
    if name.is_empty() {
        bail!("#define missing name");
    }
    if args[name_end..].starts_with('(') {
        // Skip function-like macros (leave unexpanded as identifier).
        return Ok((format!("__fnmacro_{name}"), String::new()));
    }
    let value = args[name_end..].trim().to_owned();
    Ok((name, value))
}

fn expand_macros(line: &str, macros: &HashMap<String, String>) -> String {
    if macros.is_empty() {
        return line.to_owned();
    }
    let mut output = String::with_capacity(line.len());
    let chars: Vec<char> = line.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let ch = chars[i];
        if ch == '"' {
            output.push(ch);
            i += 1;
            while i < chars.len() {
                output.push(chars[i]);
                if chars[i] == '\\' && i + 1 < chars.len() {
                    output.push(chars[i + 1]);
                    i += 2;
                    continue;
                }
                if chars[i] == '"' {
                    i += 1;
                    break;
                }
                i += 1;
            }
            continue;
        }
        if ch == '/' && i + 1 < chars.len() && chars[i + 1] == '/' {
            output.extend(chars[i..].iter().copied());
            break;
        }
        if ch.is_ascii_alphabetic() || ch == '_' {
            let start = i;
            i += 1;
            while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if let Some(value) = macros.get(&word) {
                // Avoid infinite recursion on self-define; expand once.
                output.push_str(&expand_macros(value, macros));
            } else {
                output.push_str(&word);
            }
            continue;
        }
        output.push(ch);
        i += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn combat_preprocess_line_531() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("mudlib");
        let path = root.join("std/living/combat.c");
        let src = std::fs::read_to_string(&path).unwrap();
        let out = preprocess(&src, &path, &root).expect("preprocess");
        eprintln!("nearby:");
        for (i, l) in out.lines().enumerate().skip(525).take(15) {
            eprintln!("{:>4}|{l}", i + 1);
        }
        assert!(out.contains("functionp"));
    }
}
