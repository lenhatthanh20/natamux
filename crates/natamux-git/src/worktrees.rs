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

/// Which branch a new worktree checks out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorktreeBranch<'a> {
    /// A new branch from this ref, or from HEAD. It follows no branch: started from
    /// `origin/main`, it must not push to main.
    New(Option<&'a str>),
    /// The branch that is there, with the work it has.
    Existing,
    /// A new branch from this remote ref, which it follows (`--track`).
    Track(&'a str),
}

/// Makes the worktree on `branch`, as `how` says.
pub fn add_worktree(
    repo: &Path,
    worktree: &Path,
    branch: &str,
    how: WorktreeBranch<'_>,
) -> Result<(), GitError> {
    let branch = checked_ref(branch)?;
    let path = worktree.to_string_lossy();
    let mut args = vec!["worktree", "add"];
    match how {
        WorktreeBranch::Existing => args.extend([&*path, branch]),
        WorktreeBranch::New(base_ref) => {
            let base_ref = base_ref.map(checked_ref).transpose()?;
            args.extend(["--no-track", "-b", branch, &*path]);
            args.extend(base_ref);
        }
        WorktreeBranch::Track(remote_ref) => {
            let remote_ref = checked_ref(remote_ref)?;
            args.extend(["--track", "-b", branch, &*path, remote_ref]);
        }
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

/// Reads the output of `git worktree list --porcelain`: the path of the worktree that has branch
/// `branch` checked out, if one has.
pub fn parse_worktree_of_branch(output: &str, branch: &str) -> Option<String> {
    let wanted = format!("refs/heads/{branch}");
    let mut path = None;
    for line in output.split('\n') {
        if let Some(listed) = line.strip_prefix("worktree ") {
            path = Some(listed);
        } else if line.strip_prefix("branch ") == Some(wanted.as_str()) {
            return path.map(str::to_string);
        }
    }
    None
}

/// The worktree, the main one too, that has `branch` checked out.
pub fn worktree_of_branch(repo: &Path, branch: &str) -> Result<Option<String>, GitError> {
    let output = git(
        repo,
        &["worktree", "list", "--porcelain"],
        Some(WORKTREE_TIMEOUT),
        &[],
    )?;
    Ok(parse_worktree_of_branch(&output, branch))
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
