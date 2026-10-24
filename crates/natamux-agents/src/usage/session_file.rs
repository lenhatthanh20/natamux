//! The trust boundary for a path that comes in a hook body (`sessionFile` in `usage-service.ts`).

use std::fs;
use std::path::{Path, PathBuf};

const MAX_FILE_BYTES: u64 = 200 * 1024 * 1024;

fn is_session_name(path: &str) -> bool {
    path.ends_with(".jsonl") || path.ends_with(".json")
}

/// The real path of a session file in the home folder. None for anything else: no usage is read
/// from it.
pub fn session_file(path: &str, home: &Path) -> Option<PathBuf> {
    if !Path::new(path).is_absolute() || !is_session_name(path) {
        return None;
    }
    let real = fs::canonicalize(path).ok()?;
    let home = fs::canonicalize(home).ok()?;
    if !is_session_name(&real.to_string_lossy()) || real == home || !real.starts_with(&home) {
        return None;
    }
    let info = fs::metadata(&real).ok()?;
    (info.is_file() && info.len() <= MAX_FILE_BYTES).then_some(real)
}

#[cfg(test)]
#[path = "session_file_tests.rs"]
mod tests;
