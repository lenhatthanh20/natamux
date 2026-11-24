//! What a CI check sends: the object ids of the work, the files it holds and leaves out, the
//! remote it goes to, and the push of exactly the approved commit (F4).

use std::path::Path;

use natamux_core::js::js_trim;
use natamux_core::security::{CiMode, ci_mode};

use crate::commit_push::{CI_REF_PREFIX, push_ref, snapshot_paths};
use crate::exec::{GitError, checked_ref, git, git_bytes};

/// The files of a worktree that differ from HEAD. `untracked` files are never sent to CI; ignored
/// files are in neither list.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkingFiles {
    pub tracked: Vec<String>,
    pub untracked: Vec<String>,
}

/// What a person or the daemon approved, checked again right before the push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ApprovedPush<'a> {
    pub mode: CiMode,
    pub commit: &'a str,
    pub tree: &'a str,
    /// The push URL of origin at the approval, without userinfo.
    pub remote: &'a str,
    /// The branch of origin, under `natamux-ci/`.
    pub ref_name: &'a str,
}

fn run(worktree: &Path, args: &[&str]) -> Result<String, GitError> {
    Ok(js_trim(&git(worktree, args, None, &[])?).to_string())
}

fn names(bytes: &[u8]) -> Vec<String> {
    bytes
        .split(|byte| *byte == 0)
        .filter(|name| !name.is_empty())
        .map(|name| String::from_utf8_lossy(name).into_owned())
        .collect()
}

/// HEAD and its tree.
pub fn head_snapshot(worktree: &Path) -> Result<(String, String), GitError> {
    let commit = run(worktree, &["rev-parse", "--verify", "HEAD"])?;
    let tree = tree_of(worktree, &commit)?;
    Ok((commit, tree))
}

/// The tree of a commit.
pub fn tree_of(worktree: &Path, commit: &str) -> Result<String, GitError> {
    let commit = checked_ref(commit)?;
    run(
        worktree,
        &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
    )
}

/// The changes against HEAD (staged or not, deletions too) and the untracked files.
pub fn working_files(worktree: &Path) -> Result<WorkingFiles, GitError> {
    let tracked = git_bytes(
        worktree,
        &["diff", "--name-only", "-z", "--no-renames", "HEAD", "--"],
    )?;
    let untracked = git_bytes(
        worktree,
        &["ls-files", "-z", "--others", "--exclude-standard", "--"],
    )?;
    Ok(WorkingFiles {
        tracked: names(&tracked),
        untracked: names(&untracked),
    })
}

/// HEAD minus every remote branch. Natamux's own CI branches do not count as on the remote: they
/// hold copies of the task's work, not reviewed code.
fn not_on_a_remote() -> [String; 4] {
    [
        "HEAD".into(),
        "--not".into(),
        format!("--exclude=*/{CI_REF_PREFIX}*"),
        "--remotes".into(),
    ]
}

/// The commits of HEAD that no branch of a remote has yet.
pub fn new_commits(worktree: &Path) -> Result<u64, GitError> {
    let range = not_on_a_remote();
    let mut args = vec!["rev-list", "--count"];
    args.extend(range.iter().map(String::as_str));
    let count = run(worktree, &args)?;
    count
        .parse()
        .map_err(|_| GitError(format!("git rev-list gave {count:?}, not a count")))
}

/// The paths those commits change, each once. A merge counts only what differs from all its
/// parents, so a merged branch of the remote is not listed again.
pub fn new_commit_paths(worktree: &Path) -> Result<Vec<String>, GitError> {
    let range = not_on_a_remote();
    let mut args = vec!["log", "--cc", "--name-only", "-z"];
    args.extend(["--no-renames", "--format="]);
    args.extend(range.iter().map(String::as_str));
    let mut paths = names(&git_bytes(worktree, &args)?);
    paths.sort();
    paths.dedup();
    Ok(paths)
}

/// Every push URL of origin: a push to the name goes to each.
fn push_urls(worktree: &Path) -> Result<Vec<String>, GitError> {
    let out = run(
        worktree,
        &["remote", "get-url", "--push", "--all", "origin"],
    )?;
    Ok(out.lines().map(str::to_string).collect())
}

fn more_than_one(count: usize) -> GitError {
    refused(format!(
        "origin has {count} push URLs; a CI push goes to one."
    ))
}

/// The push URL of origin, without a user name or password. More than one is an error.
pub fn remote_url(worktree: &Path) -> Result<String, GitError> {
    match push_urls(worktree)?.as_slice() {
        [url] => Ok(without_userinfo(url)),
        urls => Err(more_than_one(urls.len())),
    }
}

/// `https://user:token@host/x` gives `https://host/x`. The scp form (`git@host:x`) has no
/// password, so it stays.
pub fn without_userinfo(url: &str) -> String {
    let Some((scheme, rest)) = url.split_once("://") else {
        return url.to_string();
    };
    let end = rest.find('/').unwrap_or(rest.len());
    match rest[..end].rfind('@') {
        Some(at) => format!("{scheme}://{}", &rest[at + 1..]),
        None => url.to_string(),
    }
}

/// HEAD plus the tracked changes, never the untracked or ignored files, with its tree. Git
/// reads only the changed files (`snapshot_paths`).
pub fn tracked_snapshot(worktree: &Path, message: &str) -> Result<(String, String), GitError> {
    let files = working_files(worktree)?;
    let commit = snapshot_paths(worktree, message, &files.tracked)?;
    let tree = tree_of(worktree, &commit)?;
    Ok((commit, tree))
}

/// The local ref that keeps an approved commit until the task ends: a snapshot commit is on no
/// branch, and git may drop it otherwise.
pub fn pin_ref(task: &str, stage: &str) -> String {
    format!("refs/natamux/ci/{task}/{stage}")
}

pub fn pin(worktree: &Path, task: &str, stage: &str, commit: &str) -> Result<(), GitError> {
    let commit = checked_ref(commit)?;
    run(worktree, &["update-ref", &pin_ref(task, stage), commit]).map(drop)
}

/// Removes the pins of a task's CI checks.
pub fn unpin_task(worktree: &Path, task: &str) -> Result<(), GitError> {
    let prefix = format!("refs/natamux/ci/{task}/");
    let listed = run(worktree, &["for-each-ref", "--format=%(refname)", &prefix])?;
    for name in listed.lines().filter(|name| name.starts_with(&prefix)) {
        run(worktree, &["update-ref", "-d", name])?;
    }
    Ok(())
}

fn refused(reason: String) -> GitError {
    GitError(format!("CI push refused: {reason}"))
}

/// Checks an approval against the worktree now, with no network call: the mode still allows the
/// push, origin has one push URL and it is the approved remote, the commit holds the approved
/// tree, and the branch is a CI branch.
pub fn verify_approved(worktree: &Path, push: &ApprovedPush) -> Result<(), GitError> {
    approved_url(worktree, push).map(drop)
}

/// `verify_approved`, giving the push URL it checked (with its userinfo, which git may need).
fn approved_url(worktree: &Path, push: &ApprovedPush) -> Result<String, GitError> {
    let now = ci_mode();
    if push.mode == CiMode::Forbid || now == CiMode::Forbid {
        return Err(refused("Strict mode forbids CI pushes.".into()));
    }
    if push.mode != now {
        return Err(refused(format!(
            "the CI push mode changed from {} to {} since the approval.",
            push.mode.as_str(),
            now.as_str()
        )));
    }
    if !push.ref_name.starts_with(CI_REF_PREFIX) {
        return Err(refused(format!(
            "{:?} is not a CI branch ({CI_REF_PREFIX}...).",
            push.ref_name
        )));
    }
    checked_ref(push.ref_name)?;
    // A repository without origin has no URL; the push then fails on its own.
    let urls = push_urls(worktree).unwrap_or_default();
    if urls.len() > 1 {
        return Err(more_than_one(urls.len()));
    }
    let url = urls.into_iter().next().unwrap_or_default();
    let remote = without_userinfo(&url);
    if remote != push.remote {
        return Err(refused(format!(
            "the remote changed since the approval (approved {:?}, now {remote:?}).",
            push.remote
        )));
    }
    let tree = tree_of(worktree, push.commit)?;
    if tree != push.tree {
        return Err(refused(format!(
            "commit {} does not hold the approved tree.",
            push.commit
        )));
    }
    Ok(url)
}

/// Pushes exactly the approved commit to its CI branch, after `verify_approved`, to the one URL
/// it checked.
pub fn push_approved(worktree: &Path, push: &ApprovedPush) -> Result<(), GitError> {
    let url = approved_url(worktree, push)?;
    push_ref(worktree, push.commit, push.ref_name, &url)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn takes_the_user_and_password_out_of_a_url() {
        assert_eq!(
            without_userinfo("https://user:FAKE-TOKEN@example.com/o/r.git"),
            "https://example.com/o/r.git"
        );
        assert_eq!(
            without_userinfo("ssh://git@example.com:22/o/r.git"),
            "ssh://example.com:22/o/r.git"
        );
        assert_eq!(
            without_userinfo("https://example.com/a@b/r.git"),
            "https://example.com/a@b/r.git"
        );
        assert_eq!(
            without_userinfo("git@example.com:o/r.git"),
            "git@example.com:o/r.git"
        );
        assert_eq!(without_userinfo("/tmp/origin.git"), "/tmp/origin.git");
    }
}
