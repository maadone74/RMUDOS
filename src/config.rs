use anyhow::{bail, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DriverConfig {
    pub mud_name: String,
    pub bind: String,
    pub port: u16,
    pub mudlib: PathBuf,
    pub master: String,
    pub max_cost: usize,
}

impl Default for DriverConfig {
    fn default() -> Self {
        Self {
            mud_name: "RustMud".to_owned(),
            bind: "0.0.0.0".to_owned(),
            port: 4000,
            mudlib: PathBuf::from("mudlib"),
            master: "/secure/master".to_owned(),
            max_cost: 1_000_000,
        }
    }
}

impl DriverConfig {
    pub fn from_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        let source = fs::read_to_string(path)
            .with_context(|| format!("failed to read configuration {}", path.display()))?;
        let mut config = Self::parse(&source)?;
        if config.mudlib.is_relative() {
            let parent = path.parent().unwrap_or_else(|| Path::new("."));
            config.mudlib = parent.join(&config.mudlib);
        }
        Ok(config)
    }

    pub fn parse(source: &str) -> Result<Self> {
        let mut config = Self::default();
        for (index, raw_line) in source.lines().enumerate() {
            let line_number = index + 1;
            let line = strip_comment(raw_line).trim();
            if line.is_empty() {
                continue;
            }
            let Some((raw_key, raw_value)) = line.split_once('=') else {
                bail!("configuration line {line_number}: expected key = value");
            };
            let key = raw_key.trim();
            let value = unquote(raw_value.trim())
                .with_context(|| format!("configuration line {line_number}"))?;
            match key {
                "mud_name" => config.mud_name = value,
                "bind" => config.bind = value,
                "port" => {
                    config.port = value
                        .parse()
                        .with_context(|| format!("configuration line {line_number}: invalid port"))?
                }
                "mudlib" => config.mudlib = PathBuf::from(value),
                "master" => config.master = normalize_object_path(&value),
                "max_cost" => {
                    config.max_cost = value.parse().with_context(|| {
                        format!("configuration line {line_number}: invalid max_cost")
                    })?
                }
                _ => bail!("configuration line {line_number}: unknown key {key:?}"),
            }
        }
        if config.mud_name.is_empty() {
            bail!("mud_name may not be empty");
        }
        if config.bind.is_empty() {
            bail!("bind may not be empty");
        }
        if config.max_cost == 0 {
            bail!("max_cost must be greater than zero");
        }
        Ok(config)
    }

    pub fn socket_address(&self) -> String {
        format!("{}:{}", self.bind, self.port)
    }
}

fn strip_comment(line: &str) -> &str {
    let mut quoted = false;
    let mut escaped = false;
    for (offset, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            '#' if !quoted => return &line[..offset],
            _ => {}
        }
    }
    line
}

fn unquote(value: &str) -> Result<String> {
    if !value.starts_with('"') {
        return Ok(value.trim().to_owned());
    }
    if value.len() < 2 || !value.ends_with('"') {
        bail!("unterminated quoted value");
    }
    let mut result = String::new();
    let mut chars = value[1..value.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            result.push(ch);
            continue;
        }
        match chars.next() {
            Some('n') => result.push('\n'),
            Some('r') => result.push('\r'),
            Some('t') => result.push('\t'),
            Some('"') => result.push('"'),
            Some('\\') => result.push('\\'),
            Some(other) => {
                result.push('\\');
                result.push(other);
            }
            None => bail!("trailing escape in quoted value"),
        }
    }
    Ok(result)
}

pub(crate) fn normalize_object_path(path: &str) -> String {
    let mut path = path.trim().replace('\\', "/");
    if !path.starts_with('/') {
        path.insert(0, '/');
    }
    if path.ends_with(".c") {
        path.truncate(path.len() - 2);
    }
    while path.contains("//") {
        path = path.replace("//", "/");
    }
    path
}
