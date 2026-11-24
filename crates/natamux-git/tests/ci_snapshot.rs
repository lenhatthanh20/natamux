//! What a CI check sends (F4), against a bare local remote: a tracked change goes, an untracked
//! fake secret and an ignored file never do, and a push is exactly the approved commit.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

use natamux_core::security::{CiMode, CiPush, SecurityMode, StrictSettings, force};
use natamux_git::ci_snapshot::{
    ApprovedPush, head_snapshot, pin, push_approved, remote_url, tracked_snapshot, unpin_task,
    working_files,
};
use natamux_git::commit_push::delete_remote_ref;

const CI_REF: &str = "natamux-ci/natamux/t-0001-fix";

static COUNTER: AtomicUsize = AtomicUsize::new(0);

struct Repo {
    root: PathBuf,
    work: PathBuf,
    origin: PathBuf,
}

impl Drop for Repo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn git(cwd: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(output.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn bare(root: &Path, name: &str) -> PathBuf {
    let path = root.join(name);
    git(root, &["init", "--bare", "-q", path.to_str().unwrap()]);
    path
}

/// A repository with a committed `tracked.rs` and `.gitignore`, then a change to `tracked.rs`,
/// an untracked fake secret and an ignored file.
fn repo() -> Repo {
    let root = std::env::temp_dir().join(format!(
        "natamux-ci-snapshot-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let root = if cfg!(windows) {
        root
    } else {
        root.canonicalize().unwrap()
    };
    let work = root.join("work");
    std::fs::create_dir(&work).unwrap();
    git(&work, &["init", "-q", "-b", "main"]);
    git(&work, &["config", "core.autocrlf", "false"]);
    std::fs::write(work.join("tracked.rs"), "fn old() {}\n").unwrap();
    std::fs::write(work.join(".gitignore"), "ignored.log\n").unwrap();
    git(&work, &["add", "."]);
    git(&work, &["commit", "-q", "-m", "init"]);
    let origin = bare(&root, "origin.git");
    git(
        &work,
        &["remote", "add", "origin", origin.to_str().unwrap()],
    );
    std::fs::write(work.join("tracked.rs"), "fn new() {}\n").unwrap();
    std::fs::write(work.join("secret.txt"), "FAKE-SECRET-123\n").unwrap();
    std::fs::write(work.join("ignored.log"), "noise\n").unwrap();
    Repo { root, work, origin }
}

fn remote_names(origin: &Path) -> Vec<String> {
    git(origin, &["ls-tree", "-r", "--name-only", CI_REF])
        .lines()
        .map(String::from)
        .collect()
}

fn refs(origin: &Path) -> String {
    git(origin, &["for-each-ref", "--format=%(refname)"])
}

fn strict(ci_push: CiPush) -> SecurityMode {
    SecurityMode::Strict(StrictSettings {
        ci_push,
        ..StrictSettings::default()
    })
}

fn push<'a>(mode: CiMode, commit: &'a str, tree: &'a str, remote: &'a str) -> ApprovedPush<'a> {
    ApprovedPush {
        mode,
        commit,
        tree,
        remote,
        ref_name: CI_REF,
    }
}

#[test]
fn lists_the_tracked_changes_and_the_untracked_files_but_not_the_ignored_ones() {
    let r = repo();
    let files = working_files(&r.work).unwrap();
    assert_eq!(files.tracked, ["tracked.rs"]);
    assert_eq!(files.untracked, ["secret.txt"]);
}

#[test]
fn the_default_snapshot_sends_the_tracked_change_and_never_untracked_or_ignored_files() {
    let r = repo();
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();

    push_approved(&r.work, &push(CiMode::Snapshot, &commit, &tree, &remote)).unwrap();

    let names = remote_names(&r.origin);
    assert!(names.contains(&"tracked.rs".to_string()), "{names:?}");
    assert!(!names.contains(&"secret.txt".to_string()));
    assert!(!names.contains(&"ignored.log".to_string()));
    let content = git(&r.origin, &["show", &format!("{CI_REF}:tracked.rs")]);
    assert_eq!(content, "fn new() {}");
    assert_eq!(git(&r.origin, &["rev-parse", CI_REF]), commit);
}

#[test]
fn approved_commit_sends_head_and_nothing_uncommitted() {
    let r = repo();
    let _strict = force(strict(CiPush::ApprovedCommit));
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = head_snapshot(&r.work).unwrap();

    let approved = push(CiMode::ApprovedCommit, &commit, &tree, &remote);
    push_approved(&r.work, &approved).unwrap();

    let pushed_tree = git(&r.origin, &["rev-parse", &format!("{CI_REF}^{{tree}}")]);
    assert_eq!(pushed_tree, git(&r.work, &["rev-parse", "HEAD^{tree}"]));
    let content = git(&r.origin, &["show", &format!("{CI_REF}:tracked.rs")]);
    assert_eq!(content, "fn old() {}");
}

#[test]
fn approved_snapshot_sends_the_approved_content_even_after_a_later_edit() {
    let r = repo();
    let _strict = force(strict(CiPush::ApprovedSnapshot));
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();
    pin(&r.work, "T-0001", "implement", &commit).unwrap();
    std::fs::write(r.work.join("tracked.rs"), "fn later() {}\n").unwrap();

    let approved = push(CiMode::ApprovedSnapshot, &commit, &tree, &remote);
    push_approved(&r.work, &approved).unwrap();

    let content = git(&r.origin, &["show", &format!("{CI_REF}:tracked.rs")]);
    assert_eq!(content, "fn new() {}");
    assert!(!remote_names(&r.origin).contains(&"secret.txt".to_string()));
    unpin_task(&r.work, "T-0001").unwrap();
    assert_eq!(
        git(&r.work, &["for-each-ref", "refs/natamux/ci/T-0001/"]),
        ""
    );
}

#[test]
fn refuses_a_push_after_origin_moved_and_neither_remote_gets_a_ref() {
    let r = repo();
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();
    let other = bare(&r.root, "other.git");
    git(
        &r.work,
        &["remote", "set-url", "origin", other.to_str().unwrap()],
    );

    let error = push_approved(&r.work, &push(CiMode::Snapshot, &commit, &tree, &remote));

    let text = error.unwrap_err().0;
    assert!(text.contains("remote changed"), "{text}");
    assert_eq!(refs(&r.origin), "");
    assert_eq!(refs(&other), "");
}

#[test]
fn refuses_a_push_when_origin_has_a_second_push_url_and_neither_gets_a_ref() {
    let r = repo();
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();
    let other = bare(&r.root, "other.git");
    for url in [&r.origin, &other] {
        let url = url.to_str().unwrap();
        git(&r.work, &["config", "--add", "remote.origin.pushurl", url]);
    }

    let error = push_approved(&r.work, &push(CiMode::Snapshot, &commit, &tree, &remote));

    let text = error.unwrap_err().0;
    assert!(text.contains("2 push URLs"), "{text}");
    assert!(remote_url(&r.work).is_err());
    assert_eq!(refs(&r.origin), "");
    assert_eq!(refs(&other), "");
}

#[test]
fn refuses_a_changed_mode_a_tree_that_differs_and_a_branch_outside_natamux_ci() {
    let r = repo();
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();
    let (head, head_tree) = head_snapshot(&r.work).unwrap();

    let other_mode = push(CiMode::ApprovedCommit, &head, &head_tree, &remote);
    let error = push_approved(&r.work, &other_mode).unwrap_err();
    assert!(error.0.contains("mode changed"), "{}", error.0);
    let wrong_tree = push(CiMode::Snapshot, &head, &tree, &remote);
    let error = push_approved(&r.work, &wrong_tree).unwrap_err();
    assert!(error.0.contains("approved tree"), "{}", error.0);
    let mut outside = push(CiMode::Snapshot, &commit, &tree, &remote);
    outside.ref_name = "main";
    let error = push_approved(&r.work, &outside).unwrap_err();
    assert!(error.0.contains("not a CI branch"), "{}", error.0);
    {
        let _forbid = force(strict(CiPush::Forbid));
        let forbidden = push(CiMode::Forbid, &commit, &tree, &remote);
        let error = push_approved(&r.work, &forbidden).unwrap_err();
        assert!(error.0.contains("forbids"), "{}", error.0);
    }
    assert_eq!(refs(&r.origin), "");
}

#[test]
fn a_retry_with_a_dirty_worktree_pushes_the_approved_commit_again() {
    let r = repo();
    let remote = remote_url(&r.work).unwrap();
    let (commit, tree) = tracked_snapshot(&r.work, "Natamux CI check").unwrap();
    let approved = push(CiMode::Snapshot, &commit, &tree, &remote);
    push_approved(&r.work, &approved).unwrap();
    std::fs::write(r.work.join("tracked.rs"), "fn dirty() {}\n").unwrap();
    std::fs::write(r.work.join("more.txt"), "x\n").unwrap();

    push_approved(&r.work, &approved).unwrap();

    assert_eq!(git(&r.origin, &["rev-parse", CI_REF]), commit);
}

#[test]
fn deletes_only_ci_branches_of_origin() {
    let r = repo();
    let (head, _) = head_snapshot(&r.work).unwrap();
    let target = format!("{head}:refs/heads/main");
    git(&r.work, &["push", "-q", "origin", &target]);

    let error = delete_remote_ref(&r.work, "main").unwrap_err();

    assert!(error.0.contains("only CI branches"), "{}", error.0);
    assert_eq!(git(&r.origin, &["rev-parse", "main"]), head);
}

#[test]
fn reads_the_remote_without_its_password() {
    let r = repo();
    let url = "https://user:FAKE-TOKEN@example.com/o/r.git";
    git(&r.work, &["remote", "set-url", "origin", url]);
    assert_eq!(remote_url(&r.work).unwrap(), "https://example.com/o/r.git");
}
