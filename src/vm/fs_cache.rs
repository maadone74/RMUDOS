//! Short-lived MudOS-style stat / directory cache for `file_size` and `get_dir`.
//!
//! LPC often stats the same path many times in one command. Caching avoids
//! repeating syscalls; mutating efuns invalidate the affected paths.

use parking_lot::Mutex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const TTL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug)]
pub enum CachedStat {
    Missing,
    File { size: u64, mtime: i64 },
    Dir { mtime: i64 },
}

impl CachedStat {
    /// MudOS `file_size`: bytes, `-2` if directory, `-1` if missing.
    pub fn file_size(&self) -> i64 {
        match self {
            CachedStat::Missing => -1,
            CachedStat::Dir { .. } => -2,
            CachedStat::File { size, .. } => *size as i64,
        }
    }

    pub fn mtime(&self) -> i64 {
        match self {
            CachedStat::Missing => 0,
            CachedStat::Dir { mtime } | CachedStat::File { mtime, .. } => *mtime,
        }
    }

    pub fn exists(&self) -> bool {
        !matches!(self, CachedStat::Missing)
    }

    pub fn is_dir(&self) -> bool {
        matches!(self, CachedStat::Dir { .. })
    }
}

struct Inner {
    stats: HashMap<PathBuf, (Instant, CachedStat)>,
    dirs: HashMap<PathBuf, (Instant, Vec<String>)>,
}

pub struct FsCache {
    inner: Mutex<Inner>,
}

impl Default for FsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl FsCache {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(Inner {
                stats: HashMap::new(),
                dirs: HashMap::new(),
            }),
        }
    }

    pub fn stat(&self, path: &Path) -> CachedStat {
        if let Some(cached) = self.cached_stat(path) {
            return cached;
        }
        let value = stat_uncached(path);
        self.inner
            .lock()
            .stats
            .insert(path.to_path_buf(), (Instant::now(), value.clone()));
        value
    }

    pub fn list_dir(&self, path: &Path) -> Option<Vec<String>> {
        {
            let inner = self.inner.lock();
            if let Some((at, names)) = inner.dirs.get(path) {
                if at.elapsed() < TTL {
                    return Some(names.clone());
                }
            }
        }
        let mut names = Vec::new();
        let entries = fs::read_dir(path).ok()?;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if name == "." || name == ".." {
                continue;
            }
            names.push(name.to_owned());
        }
        names.sort();
        self.inner
            .lock()
            .dirs
            .insert(path.to_path_buf(), (Instant::now(), names.clone()));
        Some(names)
    }

    pub fn invalidate(&self, path: &Path) {
        let mut inner = self.inner.lock();
        inner.stats.remove(path);
        inner.dirs.remove(path);
        if let Some(parent) = path.parent() {
            inner.stats.remove(parent);
            inner.dirs.remove(parent);
        }
    }

    fn cached_stat(&self, path: &Path) -> Option<CachedStat> {
        let inner = self.inner.lock();
        let (at, value) = inner.stats.get(path)?;
        (at.elapsed() < TTL).then(|| value.clone())
    }
}

fn stat_uncached(path: &Path) -> CachedStat {
    match fs::metadata(path) {
        Ok(meta) => {
            let mtime = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            if meta.is_dir() {
                CachedStat::Dir { mtime }
            } else {
                CachedStat::File {
                    size: meta.len(),
                    mtime,
                }
            }
        }
        Err(_) => CachedStat::Missing,
    }
}

/// Used by tests that want a known mtime without going through the efun.
#[allow(dead_code)]
pub fn unix_mtime(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
