//! Ports `packages/daemon/src/git/commit-push.ts`: commit, push and pull, the CI snapshot, and the
//! diff that the commit drafter reads.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use natamux_core::js::{js_trim, utf16_len};

use crate::exec::{GITHUB_CREDENTIALS, GitError, checked_ref, git, with_git_config};
use crate::graph::FIELD;

/// The text of `NothingToCommitError`.
pub const NOTHING_TO_COMMIT: &str =
    "There is nothing to commit. The workspace matches its last commit.";

/// A push or a pull can take a while, but one that hangs must not hold the stage forever.
const PUSH_TIMEOUT: Duration = Duration::from_secs(2 * 60);

/// What `commit_all` and `read_commit_diff` fail with: git, or `NothingToCommitError`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CommitError {
    Git(GitError),
    NothingToCommit,
}

impl std::fmt::Display for CommitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Git(error) => std::fmt::Display::fmt(error, f),
            Self::NothingToCommit => f.write_str(NOTHING_TO_COMMIT),
        }
    }
}

impl std::error::Error for CommitError {}

impl From<GitError> for CommitError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommitResult {
    pub hash: String,
    pub subject: String,
}

fn run(cwd: &Path, args: &[&str]) -> Result<String, GitError> {
    git(cwd, args, None, &[])
}

/// Commits every change of the worktree, files that git does not track yet included.
pub fn commit_all(worktree: &Path, message: &str) -> Result<CommitResult, CommitError> {
    run(worktree, &["add", "--all"])?;
    let staged = run(worktree, &["diff", "--cached", "--name-only"])?;
    if js_trim(&staged).is_empty() {
        return Err(CommitError::NothingToCommit);
    }
    run(worktree, &["commit", "--quiet", "-m", message])?;
    let log = run(worktree, &["log", "-1", "--format=%H%x1f%s"])?;
    let mut parts = js_trim(&log).split(FIELD);
    Ok(CommitResult {
        hash: parts.next().unwrap_or("").to_string(),
        subject: parts.next().unwrap_or("").to_string(),
    })
}

/// A remote call: when git asks for a password for github.com, it asks the GitHub CLI, which uses
/// GH_TOKEN or the login of `gh auth login`. Helpers that are set up already come first.
fn remote(worktree: &Path, args: &[&str]) -> Result<(), GitError> {
    let env = with_git_config(
        std::env::var("GIT_CONFIG_COUNT").ok().as_deref(),
        &GITHUB_CREDENTIALS,
    );
    git(worktree, args, Some(PUSH_TIMEOUT), &env)?;
    Ok(())
}

pub fn push_branch(worktree: &Path, branch: &str) -> Result<(), GitError> {
    let branch = checked_ref(branch)?;
    remote(worktree, &["push", "--set-upstream", "origin", branch])
}

/// Brings in the new commits of the branch's upstream, such as the merged pull requests of
/// subtasks. False when the branch has no upstream. Fails when the two went different ways.
pub fn pull_fast_forward(worktree: &Path) -> Result<bool, GitError> {
    if run(worktree, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_err() {
        return Ok(false);
    }
    remote(worktree, &["pull", "--ff-only", "--quiet"])?;
    Ok(true)
}

static INDEX_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// A throwaway index file in the worktree's git dir, removed when this goes out of scope, on
/// every path. Not in the shared temp folder: another user could plant a file at a guessed name.
struct TempIndex(PathBuf);

impl TempIndex {
    fn new(worktree: &Path) -> Result<TempIndex, GitError> {
        let git_dir = run(worktree, &["rev-parse", "--absolute-git-dir"])?;
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_nanos());
        let name = format!(
            "natamux-index-{}-{}-{nanos}",
            std::process::id(),
            INDEX_COUNTER.fetch_add(1, Ordering::SeqCst)
        );
        Ok(TempIndex(PathBuf::from(js_trim(&git_dir)).join(name)))
    }

    fn env(&self) -> (String, String) {
        (
            "GIT_INDEX_FILE".into(),
            self.0.to_string_lossy().into_owned(),
        )
    }
}

impl Drop for TempIndex {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// A commit of the worktree as it is now, with the files that are not committed, made without
/// touching its branch, its index or its files. Ignored files stay out. With no change it is HEAD
/// itself, so a CI check finds the runs that HEAD has already, for example as a PR's head.
pub fn snapshot_commit(worktree: &Path, message: &str) -> Result<String, GitError> {
    let index = TempIndex::new(worktree)?;
    let env = [index.env()];
    git(worktree, &["read-tree", "HEAD"], None, &env)?;
    git(worktree, &["add", "--all"], None, &env)?;
    let tree = js_trim(&git(worktree, &["write-tree"], None, &env)?).to_string();
    let heads = run(worktree, &["rev-parse", "HEAD", "HEAD^{tree}"])?;
    let mut lines = js_trim(&heads).split('\n');
    let (head, head_tree) = (lines.next(), lines.next());
    if let (Some(head), Some(head_tree)) = (head, head_tree)
        && tree == head_tree
    {
        return Ok(head.to_string());
    }
    // A throwaway commit, so it needs no identity of the user's.
    let commit = [
        "-c",
        "user.name=Natamux",
        "-c",
        "user.email=natamux@localhost",
        "commit-tree",
        &tree,
        "-p",
        "HEAD",
        "-m",
        message,
    ];
    Ok(js_trim(&run(worktree, &commit)?).to_string())
}

/// The longest start of `text` that is at most `units` UTF-16 units and ends at a character, with
/// its length in units. Half of an emoji is not a character: the cut goes before it.
pub(crate) fn utf16_prefix(text: &str, units: usize) -> (&str, usize) {
    let mut used = 0;
    for (byte, c) in text.char_indices() {
        if used + c.len_utf16() > units {
            return (&text[..byte], used);
        }
        used += c.len_utf16();
    }
    (text, used)
}

/// What `commit_all` would commit now: `--stat` in full, then the diff, cut so both fit in
/// `max_chars`. Uses its own index, so the branch, the index and the files of the worktree stay
/// as they are.
pub fn read_commit_diff(worktree: &Path, max_chars: usize) -> Result<String, CommitError> {
    let index = TempIndex::new(worktree)?;
    // Paths as they are, not in octal escapes, so the agent can read them.
    let mut env = with_git_config(
        std::env::var("GIT_CONFIG_COUNT").ok().as_deref(),
        &[("core.quotePath", "false")],
    );
    env.push(index.env());
    if git(worktree, &["read-tree", "HEAD"], None, &env).is_err() {
        // A repository without a commit has no HEAD: everything is new.
        git(worktree, &["read-tree", "--empty"], None, &env)?;
    }
    git(worktree, &["add", "--all"], None, &env)?;
    // Without HEAD, `diff --cached` shows every staged file.
    let diff = ["diff", "--cached", "--no-color", "--no-ext-diff", "-M"];
    let mut stat_args = diff.to_vec();
    stat_args.push("--stat");
    let stat = git(worktree, &stat_args, None, &env)?;
    if js_trim(&stat).is_empty() {
        return Err(CommitError::NothingToCommit);
    }
    let full = git(worktree, &diff, None, &env)?;
    let room = max_chars.saturating_sub(utf16_len(&stat) + 1);
    let full_len = utf16_len(&full);
    if full_len <= room {
        return Ok(format!("{stat}\n{full}"));
    }
    let (kept, end) = utf16_prefix(&full, room);
    Ok(format!(
        "{stat}\n{kept}\n[diff cut: {} more characters]",
        full_len - end
    ))
}

/// Puts a commit on a branch of origin, in place of what is there.
pub fn push_ref(worktree: &Path, sha: &str, ref_name: &str) -> Result<(), GitError> {
    let target = format!("{sha}:refs/heads/{ref_name}");
    remote(worktree, &["push", "--force", "--quiet", "origin", &target])
}

pub fn delete_remote_ref(cwd: &Path, ref_name: &str) -> Result<(), GitError> {
    remote(cwd, &["push", "--quiet", "origin", "--delete", ref_name])
}

#[cfg(test)]
#[path = "commit_push_tests.rs"]
mod tests;
