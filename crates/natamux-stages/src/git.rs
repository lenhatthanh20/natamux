//! Ports the git calls of stages: `currentHead` of `stage-checks.ts`, the `git` helper and
//! `withGitConfig` of `git/exec.ts`, `pullFastForward` of `git/commit-push.ts`, and the
//! `rev-list`, `status` and `rev-parse --git-common-dir` calls of the stage checks and files.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use crate::process::capture;

/// `MAX_OUTPUT_BYTES` of `git/exec.ts`.
pub const MAX_OUTPUT_BYTES: usize = 10 * 1024 * 1024;
/// `PUSH_TIMEOUT_MS`: a pull that hangs must not hold the stage forever.
pub const PULL_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// `GITHUB_CREDENTIALS`: when git asks for a password for github.com, it asks the GitHub CLI.
pub const GITHUB_CREDENTIALS: [(&str, &str); 1] = [(
    "credential.https://github.com.helper",
    "!gh auth git-credential",
)];

/// `GitError`: `git <command> failed: <detail>`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitError(pub String);

impl std::fmt::Display for GitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for GitError {}

/// `withGitConfig`: the `GIT_CONFIG_*` variables that add `entries` after the ones the
/// environment has (`current_count` is its `GIT_CONFIG_COUNT`).
pub fn with_git_config(
    current_count: Option<&str>,
    entries: &[(&str, &str)],
) -> Vec<(String, String)> {
    let start: usize = current_count
        .and_then(|count| count.trim().parse().ok())
        .unwrap_or(0);
    let mut env = vec![(
        "GIT_CONFIG_COUNT".to_string(),
        (start + entries.len()).to_string(),
    )];
    for (offset, (key, value)) in entries.iter().enumerate() {
        env.push((
            format!("GIT_CONFIG_KEY_{}", start + offset),
            key.to_string(),
        ));
        env.push((
            format!("GIT_CONFIG_VALUE_{}", start + offset),
            value.to_string(),
        ));
    }
    env
}

/// Runs git with an argument list, never a shell. `timeout` None waits for the end.
pub(crate) fn git(
    cwd: &Path,
    args: &[&str],
    timeout: Option<Duration>,
    env: &[(String, String)],
) -> Result<String, GitError> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        .env("GIT_TERMINAL_PROMPT", "0")
        .envs(env.iter().map(|(key, value)| (key, value)));
    let ran = capture(
        command,
        timeout.unwrap_or(Duration::from_secs(60 * 60 * 24 * 365)),
        MAX_OUTPUT_BYTES,
    );
    let fail = |detail: String| {
        GitError(format!(
            "git {} failed: {detail}",
            args.first().unwrap_or(&"")
        ))
    };
    if ran.not_found {
        return Err(fail("git is not installed or not on the PATH".into()));
    }
    if ran.code == Some(0) && !ran.overflow {
        return Ok(ran.stdout);
    }
    let stderr = natamux_core::js::js_trim(&ran.stderr);
    if !stderr.is_empty() {
        return Err(fail(stderr.to_string()));
    }
    Err(fail(format!("Command failed: git {}", args.join(" "))))
}

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

/// Brings in the new commits of the branch's upstream, such as the merged pull requests of
/// subtasks. False when the branch has no upstream. Fails when the two went different ways.
pub fn pull_fast_forward(worktree: &Path) -> Result<bool, GitError> {
    if git(
        worktree,
        &["rev-parse", "--abbrev-ref", "@{upstream}"],
        None,
        &[],
    )
    .is_err()
    {
        return Ok(false);
    }
    let env = with_git_config(
        std::env::var("GIT_CONFIG_COUNT").ok().as_deref(),
        &GITHUB_CREDENTIALS,
    );
    git(
        worktree,
        &["pull", "--ff-only", "--quiet"],
        Some(PULL_TIMEOUT),
        &env,
    )?;
    Ok(true)
}

#[cfg(test)]
#[path = "git_tests.rs"]
mod tests;
