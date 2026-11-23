//! Which files of a worktree git tracks: those came from the repo, never from Natamux.

use std::collections::HashSet;
use std::path::Path;

use crate::exec::{GitError, git_bytes};

/// The ones of `paths` (relative, with `/`) that git tracks in `worktree`. A folder in `paths`
/// gives the tracked files under it.
pub fn tracked_paths(worktree: &Path, paths: &[String]) -> Result<HashSet<String>, GitError> {
    if paths.is_empty() {
        return Ok(HashSet::new());
    }
    let mut args = vec!["ls-files", "-z", "--full-name", "--"];
    args.extend(paths.iter().map(String::as_str));
    let out = git_bytes(worktree, &args)?;
    Ok(out
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect())
}

#[cfg(test)]
#[path = "tracked_tests.rs"]
mod tests;
