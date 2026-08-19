pub mod ast;
pub mod codegen;
pub mod lexer;
pub mod parser;
pub mod preprocess;

use crate::config::normalize_object_path;
use crate::vm::program::Program;
use anyhow::{bail, Context, Result};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

pub fn compile_source(source: &str, path: &str) -> Result<Program> {
    let ast = parser::parse(source).with_context(|| format!("while parsing {path}"))?;
    if !ast.inherits.is_empty() {
        bail!("compile_source cannot resolve inherits; use compile_file");
    }
    codegen::generate(ast, path, Vec::new())
        .with_context(|| format!("while generating bytecode for {path}"))
}

pub fn compile_file(path: impl AsRef<Path>) -> Result<Arc<Program>> {
    let path = path.as_ref();
    let root = discover_mudlib_root(path);
    let relative = path
        .strip_prefix(&root)
        .with_context(|| format!("{} is outside {}", path.display(), root.display()))?;
    let mut object_path = format!("/{}", relative.to_string_lossy().replace('\\', "/"));
    object_path = normalize_object_path(&object_path);
    compile_file_in(root, &object_path)
}

pub fn compile_file_in(
    mudlib_root: impl AsRef<Path>,
    object_path: &str,
) -> Result<Arc<Program>> {
    let mut cache = HashMap::new();
    let mut visiting = HashSet::new();
    compile_recursive(
        mudlib_root.as_ref(),
        &normalize_object_path(object_path),
        &mut cache,
        &mut visiting,
    )
}

/// True when `{object_path}.c` exists under the mudlib (MudOS virtual compile
/// is only for paths with no source file).
pub fn source_exists(mudlib_root: impl AsRef<Path>, object_path: &str) -> bool {
    object_file(mudlib_root.as_ref(), &normalize_object_path(object_path))
        .ok()
        .is_some_and(|path| path.is_file())
}

fn compile_recursive(
    root: &Path,
    object_path: &str,
    cache: &mut HashMap<String, Arc<Program>>,
    visiting: &mut HashSet<String>,
) -> Result<Arc<Program>> {
    if let Some(program) = cache.get(object_path) {
        return Ok(program.clone());
    }
    if !visiting.insert(object_path.to_owned()) {
        bail!("cyclic inheritance involving {object_path}");
    }
    let file_path = object_file(root, object_path)?;
    tracing::info!(path = %object_path, file = %file_path.display(), "compile start");
    let started = std::time::Instant::now();
    let source = fs::read_to_string(&file_path)
        .with_context(|| format!("failed to read LPC object {}", file_path.display()))?;
    let source = preprocess::preprocess(&source, &file_path, root)
        .with_context(|| format!("while preprocessing {object_path}"))?;
    let ast = parser::parse(&source).with_context(|| format!("while parsing {object_path}"))?;
    let mut inherited = Vec::new();
    for inherit in &ast.inherits {
        let resolved = resolve_inherit(object_path, inherit)?;
        inherited.push(compile_recursive(root, &resolved, cache, visiting)?);
    }
    let program = Arc::new(
        codegen::generate(ast, object_path, inherited)
            .with_context(|| format!("while generating bytecode for {object_path}"))?,
    );
    visiting.remove(object_path);
    cache.insert(object_path.to_owned(), program.clone());
    tracing::info!(
        path = %object_path,
        elapsed_ms = started.elapsed().as_millis(),
        "compile done"
    );
    Ok(program)
}

/// MudOS source path is `{object_path}.c` (append, do not replace a virtual
/// suffix like `.armour` / `.weapon`).
fn lpc_source_path(root: &Path, object_path: &str) -> PathBuf {
    root.join(format!("{}.c", object_path.trim_start_matches('/')))
}

fn object_file(root: &Path, object_path: &str) -> Result<PathBuf> {
    let relative = object_path.trim_start_matches('/');
    let path = Path::new(relative);
    if path
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        bail!("invalid LPC object path {object_path:?}");
    }
    let primary = lpc_source_path(root, object_path);
    if primary.is_file() {
        return Ok(primary);
    }
    if let Some(alt_path) = crate::config::wizard_to_domain_path(object_path) {
        let alt = lpc_source_path(root, &alt_path);
        if alt.is_file() {
            return Ok(alt);
        }
    }
    Ok(primary)
}

fn resolve_inherit(current: &str, inherit: &str) -> Result<String> {
    let candidate = if inherit.starts_with('/') {
        PathBuf::from(inherit.trim_start_matches('/'))
    } else {
        let parent = Path::new(current.trim_start_matches('/'))
            .parent()
            .unwrap_or_else(|| Path::new(""));
        parent.join(inherit)
    };
    let mut parts = Vec::new();
    for component in candidate.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::CurDir => {}
            Component::ParentDir => {
                if parts.pop().is_none() {
                    bail!("inherit path escapes the mudlib: {inherit:?}");
                }
            }
            _ => bail!("invalid inherit path {inherit:?}"),
        }
    }
    Ok(normalize_object_path(&format!("/{}", parts.join("/"))))
}

fn discover_mudlib_root(path: &Path) -> PathBuf {
    path.ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "mudlib"))
        .map(Path::to_path_buf)
        .unwrap_or_else(|| path.parent().unwrap_or_else(|| Path::new(".")).to_path_buf())
}
