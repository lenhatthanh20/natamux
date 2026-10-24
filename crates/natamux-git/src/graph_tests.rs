use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;
use crate::test_support::{TempDir, run_git, test_repo};

pub(super) const BRANCH: &str = "natamux/t-0001-example";

pub(super) struct Fixture {
    pub(super) root: TempDir,
    pub(super) dir: PathBuf,
    pub(super) worktree: PathBuf,
}

pub(super) fn fixture() -> Fixture {
    let (root, dir) = test_repo();
    let worktree = root.path().join("worktrees").join("T-0001");
    Fixture {
        root,
        dir,
        worktree,
    }
}

pub(super) fn text(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

pub(super) fn head_of(cwd: &Path, rev: &str) -> String {
    run_git(cwd, &["rev-parse", rev]).trim().to_string()
}

pub(super) fn commit_empty(cwd: &Path, message: &str) {
    run_git(cwd, &["commit", "--allow-empty", "-m", message]);
}

// A commit that changes a file, so a rebase keeps it.
pub(super) fn commit_file(cwd: &Path, name: &str) {
    std::fs::write(cwd.join(format!("{name}.txt")), format!("{name}\n")).unwrap();
    run_git(cwd, &["add", "--all"]);
    run_git(cwd, &["commit", "-m", name]);
}

pub(super) fn add_worktree(f: &Fixture, base: Option<&str>) {
    let path = text(&f.worktree);
    let mut args = vec!["worktree", "add", "--no-track", "-b", BRANCH, &path];
    args.extend(base);
    run_git(&f.dir, &args);
}

pub(super) fn bare_origin(f: &Fixture) -> PathBuf {
    let remote = f.root.path().join("origin.git");
    let remote_text = text(&remote);
    run_git(
        f.root.path(),
        &["init", "--bare", "-b", "main", &remote_text],
    );
    run_git(&f.dir, &["remote", "add", "origin", &remote_text]);
    remote
}

pub(super) fn origin_with_main(f: &Fixture) {
    bare_origin(f);
    run_git(&f.dir, &["push", "origin", "main"]);
}

pub(super) fn graph_of(f: &Fixture, base: Option<&str>) -> GitGraph {
    read_git_graph(&f.worktree, base, MAX_GRAPH_COMMITS).unwrap()
}

#[test]
fn reads_refs() {
    let cases: [(&str, &[&str]); 5] = [
        ("", &[]),
        ("HEAD", &[]),
        ("HEAD -> natamux/t-0001", &["natamux/t-0001"]),
        (
            "HEAD -> main, tag: v1, origin/main",
            &["main", "tag: v1", "origin/main"],
        ),
        ("origin/main, origin/HEAD", &["origin/main"]),
    ];
    for (decoration, refs) in cases {
        assert_eq!(parse_refs(decoration), refs, "{decoration:?}");
    }
}

#[test]
fn reads_records_parents_refs_and_a_subject_that_holds_the_field_separator() {
    let output = [
        [
            "b2",
            "a1 c3",
            "Ann",
            "2026-10-08T10:00:00+07:00",
            "HEAD -> main",
            "Merge x\u{1f}y",
        ]
        .join(FIELD),
        ["a1", "", "Bob", "2026-10-07T10:00:00+07:00", "", "init"].join(FIELD),
        String::new(),
    ]
    .join("\0");

    assert_eq!(
        parse_log(&output, &|hash| hash == "b2"),
        vec![
            GraphCommit {
                hash: "b2".into(),
                parents: vec!["a1".into(), "c3".into()],
                author: "Ann".into(),
                date: "2026-10-08T10:00:00+07:00".into(),
                subject: "Merge x\u{1f}y".into(),
                refs: vec!["main".into()],
                on_branch: true,
            },
            GraphCommit {
                hash: "a1".into(),
                parents: vec![],
                author: "Bob".into(),
                date: "2026-10-07T10:00:00+07:00".into(),
                subject: "init".into(),
                refs: vec![],
                on_branch: false,
            },
        ]
    );
}

// A branch that is not checked out, and someone else pushed a commit to it.
fn release_behind_origin(f: &Fixture) -> String {
    let origin = bare_origin(f);
    run_git(&f.dir, &["branch", "release"]);
    run_git(&f.dir, &["push", "-u", "origin", "release"]);
    let other = f.root.path().join("other");
    let (origin_text, other_text) = (text(&origin), text(&other));
    let clone = ["clone", "--branch", "release", &origin_text, &other_text];
    run_git(f.root.path(), &clone);
    commit_empty(&other, "newer");
    run_git(&other, &["push", "origin", "release"]);
    head_of(&other, "HEAD")
}

#[test]
fn fetches_a_branch_that_is_not_checked_out_and_starts_from_its_remote_copy() {
    let f = fixture();
    let newer = release_behind_origin(&f);
    let local = head_of(&f.dir, "release");

    fetch_upstream(&f.dir, "release");

    assert_eq!(
        start_ref(&f.dir, "release").as_deref(),
        Some("origin/release")
    );
    assert_eq!(head_of(&f.dir, "origin/release"), newer);
    // The local branch is not moved.
    assert_eq!(head_of(&f.dir, "release"), local);
}

#[test]
fn starts_from_the_local_branch_when_it_has_commits_that_are_not_pushed() {
    let f = fixture();
    release_behind_origin(&f);
    run_git(&f.dir, &["checkout", "release"]);
    commit_empty(&f.dir, "local only");
    run_git(&f.dir, &["checkout", "main"]);

    fetch_upstream(&f.dir, "release");

    assert_eq!(start_ref(&f.dir, "release"), None);
}

#[test]
fn starts_from_the_local_branch_when_it_follows_no_remote_branch() {
    let f = fixture();
    run_git(&f.dir, &["branch", "release"]);

    fetch_upstream(&f.dir, "release");

    assert_eq!(start_ref(&f.dir, "release"), None);
}

// main: init, then "main moved" after the branch started. The branch: "one", then "two".
pub(super) fn diverged_worktree(f: &Fixture) {
    add_worktree(f, None);
    commit_empty(&f.worktree, "one");
    commit_empty(&f.worktree, "two");
    commit_empty(&f.dir, "main moved");
}

pub(super) fn subjects(graph: &GitGraph) -> Vec<&str> {
    graph.commits.iter().map(|c| c.subject.as_str()).collect()
}

#[test]
fn shows_the_branch_and_its_base_newest_first_and_counts_ahead_and_behind() {
    let f = fixture();
    diverged_worktree(&f);

    let graph = graph_of(&f, Some("main"));

    assert_eq!(graph.branch.as_deref(), Some(BRANCH));
    assert_eq!(graph.base.as_deref(), Some("main"));
    assert_eq!((graph.ahead, graph.behind), (Some(2), Some(1)));
    assert!(!graph.truncated);
    assert_eq!(graph.head, head_of(&f.worktree, "HEAD"));
    let mut sorted = subjects(&graph);
    sorted.sort_unstable();
    assert_eq!(sorted, ["init", "main moved", "one", "two"]);
    let by_subject: HashMap<&str, &GraphCommit> = graph
        .commits
        .iter()
        .map(|commit| (commit.subject.as_str(), commit))
        .collect();
    let two = by_subject["two"];
    assert!(two.on_branch);
    assert_eq!(two.refs, [BRANCH]);
    assert_eq!(two.author, "Test");
    assert!(by_subject["one"].on_branch);
    let moved = by_subject["main moved"];
    assert!(!moved.on_branch);
    assert_eq!(moved.refs, ["main"]);
    let init = by_subject["init"];
    assert!(!init.on_branch);
    assert!(init.parents.is_empty());
    assert_eq!(two.parents, [by_subject["one"].hash.clone()]);
    // Topological order: a commit always comes before its parents.
    let order: Vec<&str> = graph.commits.iter().map(|c| c.hash.as_str()).collect();
    let place = |hash: &str| order.iter().position(|each| *each == hash);
    for commit in &graph.commits {
        for parent in &commit.parents {
            assert!(place(parent) > place(&commit.hash));
        }
    }
}

#[test]
fn cuts_the_list_at_the_limit_and_says_so() {
    let f = fixture();
    diverged_worktree(&f);

    let graph = read_git_graph(&f.worktree, Some("main"), 2).unwrap();

    assert_eq!(graph.commits.len(), 2);
    assert!(graph.truncated);
}

#[test]
fn has_no_base_when_there_is_none_or_when_the_base_is_the_branch_itself() {
    let f = fixture();
    diverged_worktree(&f);

    for base in [None, Some(BRANCH)] {
        let graph = graph_of(&f, base);
        assert_eq!(
            (graph.base.clone(), graph.ahead, graph.behind),
            (None, None, None)
        );
        assert_eq!(subjects(&graph), ["two", "one", "init"]);
        assert!(graph.commits.iter().all(|commit| commit.on_branch));
    }
}
