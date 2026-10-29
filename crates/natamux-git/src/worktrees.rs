//! Ports `packages/daemon/src/git/worktrees.ts`: making, removing and finding worktrees.

use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::exec::{GitError, checked_ref, git, with_git_config};

// One checkout worker per CPU: a big repository checks out much faster. Git before 2.32 ignores it.
// A worktree call that hangs must end, not wait a year.
const WORKTREE_TIMEOUT: Duration = Duration::from_secs(10 * 60);

const PARALLEL_CHECKOUT: [(&str, &str); 1] = [("checkout.workers", "0")];

/// The `GIT_CONFIG_*` settings of `add_worktree`, on top of the ones the process already has.
pub fn add_worktree_env() -> Vec<(String, String)> {
    let count = std::env::var("GIT_CONFIG_COUNT").ok();
    with_git_config(count.as_deref(), &PARALLEL_CHECKOUT)
}

/// Makes a new branch for the worktree, from `base_ref` or HEAD. With `existing`, checks out the
/// branch that is there instead, with the work it has. A new branch follows no branch: started
/// from `origin/main`, it must not push to main.
pub fn add_worktree(
    repo: &Path,
    worktree: &Path,
    branch: &str,
    base_ref: Option<&str>,
    existing: bool,
) -> Result<(), GitError> {
    let branch = checked_ref(branch)?;
    let base_ref = base_ref.map(checked_ref).transpose()?;
    let path = worktree.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    if existing {
        args.extend([&*path, branch]);
    } else {
        args.extend(["--no-track", "-b", branch, &*path]);
        args.extend(base_ref);
    }
    git(repo, &args, Some(WORKTREE_TIMEOUT), &add_worktree_env()).map(|_| ())
}

/// Refuses to remove a worktree that has changes that are not committed, or submodules checked
/// out. `force` removes it either way.
pub fn remove_worktree(repo: &Path, worktree: &Path, force: bool) -> Result<(), GitError> {
    let path = worktree.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path);
    git(repo, &args, Some(WORKTREE_TIMEOUT), &[]).map(|_| ())
}

/// Whether a submodule is checked out in the worktree. Lines of submodules that are not start
/// with "-".
pub fn has_submodules(worktree: &Path) -> Result<bool, GitError> {
    let output = git(
        worktree,
        &["submodule", "status"],
        Some(WORKTREE_TIMEOUT),
        &[],
    )?;
    Ok(output
        .split('\n')
        .any(|line| !line.is_empty() && !line.starts_with('-')))
}

/// Whether git tracks a file at or below `path` in the worktree. False when git fails.
pub fn tracks_path(worktree: &Path, path: &str) -> bool {
    git(
        worktree,
        &["ls-files", "-z", "--", path],
        Some(WORKTREE_TIMEOUT),
        &[],
    )
    .is_ok_and(|output| !output.is_empty())
}

/// Reads the output of `git worktree list --porcelain`: the path of each worktree, main first.
pub fn parse_worktree_list(output: &str) -> Vec<String> {
    output
        .split('\n')
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(str::to_string)
        .collect()
}

pub fn list_worktree_paths(repo: &Path) -> Result<Vec<String>, GitError> {
    let output = git(
        repo,
        &["worktree", "list", "--porcelain"],
        Some(WORKTREE_TIMEOUT),
        &[],
    )?;
    Ok(parse_worktree_list(&output))
}

/// `Stale`: git lists the worktree but its folder is gone. `Missing`: git does not list it and the
/// folder is gone. `NotAWorktree`: the folder is there but git does not know it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorktreeState {
    Present,
    Stale,
    Missing,
    NotAWorktree,
}

impl WorktreeState {
    /// The word Node uses.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Present => "present",
            Self::Stale => "stale",
            Self::Missing => "missing",
            Self::NotAWorktree => "not-a-worktree",
        }
    }
}

// git lists real paths, so a path is resolved the same way before it is compared. A folder that
// is gone is resolved through its parent.
fn comparable_path(path: &Path) -> PathBuf {
    if let Ok(real) = path.canonicalize() {
        return real;
    }
    if let (Some(parent), Some(name)) = (path.parent(), path.file_name())
        && let Ok(real) = parent.canonicalize()
    {
        return real.join(name);
    }
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf())
}

pub fn worktree_state(repo: &Path, worktree: &Path) -> Result<WorktreeState, GitError> {
    let exists = worktree.exists();
    let wanted = comparable_path(worktree);
    let registered = list_worktree_paths(repo)?
        .iter()
        .any(|listed| comparable_path(Path::new(listed)) == wanted);
    Ok(match (exists, registered) {
        (true, true) => WorktreeState::Present,
        (true, false) => WorktreeState::NotAWorktree,
        (false, true) => WorktreeState::Stale,
        (false, false) => WorktreeState::Missing,
    })
}

#[cfg(test)]
#[path = "worktrees_tests.rs"]
mod tests;
