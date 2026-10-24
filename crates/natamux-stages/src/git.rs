//! The git calls of stages: `currentHead` of `stage-checks.ts`, and the `rev-list`, `status` and
//! `rev-parse --git-common-dir` calls of the stage checks and files. The process itself is `natamux_git::exec`.

use std::path::Path;

pub use natamux_git::commit_push::pull_fast_forward;
use natamux_git::exec::git;
pub use natamux_git::exec::{GITHUB_CREDENTIALS, GitError, MAX_OUTPUT_BYTES, with_git_config};

/// The commit HEAD is on, or None when there is none.
pub fn current_head(worktree: &Path) -> Option<String> {
    if let Some(head) = natamux_agents::git_files::head(worktree) {
        return Some(head);
    }
    git(worktree, &["rev-parse", "HEAD"], None, &[])
        .ok()
        .map(|out| natamux_core::js::js_trim(&out).to_string())
}

/// `rev-list --count <range>`.
pub fn commit_count(cwd: &Path, range: &str) -> Result<u64, GitError> {
    let out = git(cwd, &["rev-list", "--count", range], None, &[])?;
    natamux_core::js::js_trim(&out)
        .parse()
        .map_err(|_| GitError(format!("git rev-list failed: unexpected count {out:?}")))
}

/// The lines of `status --porcelain`: the files that are not committed.
pub fn dirty_files(cwd: &Path) -> Result<Vec<String>, GitError> {
    let out = git(cwd, &["status", "--porcelain"], None, &[])?;
    Ok(out
        .split('\n')
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect())
}

/// `rev-parse --git-common-dir`, trimmed. It may be relative to `cwd`.
pub fn common_dir(cwd: &Path) -> Result<String, GitError> {
    let out = git(cwd, &["rev-parse", "--git-common-dir"], None, &[])?;
    Ok(natamux_core::js::js_trim(&out).to_string())
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
