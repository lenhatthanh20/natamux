//! Ports `packages/daemon/src/git/commit-push.ts`: commit, push and pull, the CI snapshot, and the
//! diff that the commit drafter reads.

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use natamux_core::js::{js_trim, utf16_len};
use natamux_core::security::{Egress, network_for};

use crate::egress::{RemoteOp, git_remote};
use crate::exec::{
    GITHUB_CREDENTIALS, GitError, checked_ref, current_branch, git, with_git_config,
};
use crate::graph::FIELD;
use crate::rollback::path_batches;

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

/// A remote call to origin: when git asks for a password for github.com, it asks the GitHub CLI,
/// which uses GH_TOKEN or the login of `gh auth login`. Helpers that are set up already come
/// first. Under egress `allowlist` the URL is checked first (`crate::egress`).
fn remote(
    worktree: &Path,
    op: RemoteOp,
    args: &dyn Fn(&str, bool) -> Vec<String>,
) -> Result<(), GitError> {
    let env = with_git_config(
        std::env::var("GIT_CONFIG_COUNT").ok().as_deref(),
        &GITHUB_CREDENTIALS,
    );
    git_remote(worktree, op, "origin", args, Some(PUSH_TIMEOUT), &env)?;
    Ok(())
}

fn owned(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

pub fn push_branch(worktree: &Path, branch: &str) -> Result<(), GitError> {
    let branch = checked_ref(branch)?;
    let pinned = Cell::new(false);
    let target = format!("refs/heads/{branch}:refs/heads/{branch}");
    remote(worktree, RemoteOp::Push, &|to, is_pinned| {
        pinned.set(is_pinned);
        if is_pinned {
            owned(&["push", to, &target])
        } else {
            owned(&["push", "--set-upstream", "origin", branch])
        }
    })?;
    if pinned.get() {
        // A push to a URL updates no remote-tracking ref and sets no upstream by itself.
        let tracking = format!("refs/remotes/origin/{branch}");
        run(
            worktree,
            &["update-ref", &tracking, &format!("refs/heads/{branch}")],
        )?;
        run(
            worktree,
            &["config", &format!("branch.{branch}.remote"), "origin"],
        )?;
        let merge = format!("refs/heads/{branch}");
        run(
            worktree,
            &["config", &format!("branch.{branch}.merge"), &merge],
        )?;
    }
    Ok(())
}

/// Brings in the new commits of the branch's upstream, such as the merged pull requests of
/// subtasks. False when the branch has no upstream. Fails when the two went different ways.
pub fn pull_fast_forward(worktree: &Path) -> Result<bool, GitError> {
    if run(worktree, &["rev-parse", "--abbrev-ref", "@{upstream}"]).is_err() {
        return Ok(false);
    }
    if network_for(worktree).egress != Egress::Allowlist {
        remote(worktree, RemoteOp::Fetch, &|_, _| {
            owned(&["pull", "--ff-only", "--quiet"])
        })?;
        return Ok(true);
    }
    // Under the allowlist: a fetch of the checked URL, then a fast-forward to what came.
    let branch = current_branch(worktree).unwrap_or_default();
    let name = run(
        worktree,
        &["config", "--get", &format!("branch.{branch}.remote")],
    )?;
    let merge = run(
        worktree,
        &["config", "--get", &format!("branch.{branch}.merge")],
    )?;
    let (name, merge) = (js_trim(&name).to_string(), js_trim(&merge).to_string());
    let short = merge.strip_prefix("refs/heads/").unwrap_or(&merge);
    let refspec = format!("+{merge}:refs/remotes/{name}/{short}");
    // Only the pinned call runs here; it sets its own credential helper.
    git_remote(
        worktree,
        RemoteOp::Fetch,
        &name,
        &|to, _| owned(&["fetch", "--quiet", to, &refspec]),
        Some(PUSH_TIMEOUT),
        &[],
    )?;
    run(worktree, &["merge", "--ff-only", "--quiet", "FETCH_HEAD"])?;
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
    snapshot(worktree, message, |env| {
        git(worktree, &["add", "--all"], None, env).map(drop)
    })
}

/// `snapshot_commit` of only `paths`: every other file is as in HEAD. Git reads the named files
/// and no other, so the time does not grow with the worktree (`git add --all` on a new index
/// hashes every file). A path that is gone from disk is left out of the commit.
pub fn snapshot_paths(
    worktree: &Path,
    message: &str,
    paths: &[String],
) -> Result<String, GitError> {
    snapshot(worktree, message, |env| {
        for batch in path_batches(paths) {
            // `update-index` takes each name as it is, never as a pattern. Unlike `git add`, it
            // does not fail on a path that is neither on disk nor in HEAD (added, then deleted).
            let mut args = vec!["update-index", "--add", "--remove", "--"];
            args.extend(batch.iter().map(String::as_str));
            git(worktree, &args, None, env)?;
        }
        Ok(())
    })
}

// A commit on HEAD of HEAD's files and what `add` puts in the throwaway index.
fn snapshot(
    worktree: &Path,
    message: &str,
    add: impl FnOnce(&[(String, String)]) -> Result<(), GitError>,
) -> Result<String, GitError> {
    let index = TempIndex::new(worktree)?;
    let env = [index.env()];
    git(worktree, &["read-tree", "HEAD"], None, &env)?;
    add(&env)?;
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
    remote(worktree, RemoteOp::Push, &|to, _| {
        owned(&["push", "--force", "--quiet", to, &target])
    })
}

/// The branches of origin that only hold CI snapshots. Natamux pushes to and deletes only these.
pub const CI_REF_PREFIX: &str = "natamux-ci/";

/// Deletes a CI branch of origin. Any other ref is refused, so a wrong name never deletes work.
pub fn delete_remote_ref(cwd: &Path, ref_name: &str) -> Result<(), GitError> {
    if !ref_name.starts_with(CI_REF_PREFIX) {
        return Err(GitError(format!(
            "Natamux deletes only CI branches ({CI_REF_PREFIX}...), not {ref_name:?}"
        )));
    }
    remote(cwd, RemoteOp::Push, &|to, _| {
        owned(&["push", "--quiet", to, "--delete", ref_name])
    })
}

#[cfg(test)]
#[path = "commit_push_tests.rs"]
mod tests;
