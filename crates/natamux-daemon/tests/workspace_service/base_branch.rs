//! `describe('base branch')`.

use std::fs;

use natamux_proto::projects::repo::BranchList;

use super::*;

fn subject(task: &Task) -> String {
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    run_git(&workspace, &["log", "-1", "--format=%s"])
        .trim()
        .to_string()
}

fn head(task: &Task) -> String {
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    run_git(&workspace, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

#[test]
fn lists_the_local_branches_and_the_current_one() {
    let fx = Fx::new();
    run_git(&fx.repo, &["branch", "release"]);

    let expected = BranchList {
        branches: strings(&["main", "release"]),
        current: Some("main".into()),
        default_branch: Some("main".into()),
    };
    assert_eq!(fx.ws.branches(None).unwrap(), expected);
}

#[test]
fn starts_from_the_default_branch_when_none_is_given_not_from_the_checkout() {
    let fx = Fx::new();
    run_git(&fx.repo, &["checkout", "-b", "feature"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on feature"]);

    let created = fx.with_workspace("Fix the bug");

    assert_eq!(subject(&created), "init");
}

// The picked branch was not pulled: the task starts from its remote copy, fetched now.
#[test]
fn starts_a_picked_branch_from_its_newest_remote_commit() {
    let fx = Fx::new();
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["branch", "release"]);
    run_git(&fx.repo, &["push", "-u", "origin", "release"]);
    let other = fx.root.path().join("other");
    run_git(
        fx.root.path(),
        &[
            "clone",
            "--branch",
            "release",
            path_text(&origin),
            path_text(&other),
        ],
    );
    run_git(&other, &["commit", "--allow-empty", "-m", "newer"]);
    run_git(&other, &["push", "origin", "release"]);
    let newer = run_git(&other, &["rev-parse", "HEAD"]);
    let local = run_git(&fx.repo, &["rev-parse", "release"]);

    let created = fx
        .ws
        .create(&fx.task("Fresh").id, Some("release"), None)
        .unwrap();

    assert_eq!(head(&created), newer.trim());
    assert_eq!(
        run_git(&fx.repo, &["rev-parse", "release"]).trim(),
        local.trim()
    );
    let workspace = PathBuf::from(created.workspace.unwrap());
    assert!(!git_ok(&workspace, &["rev-parse", "@{upstream}"]));
}

#[test]
fn starts_a_picked_branch_from_its_local_commits_that_are_not_pushed_even_offline() {
    let fx = Fx::new();
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["checkout", "-b", "release"]);
    run_git(&fx.repo, &["push", "-u", "origin", "release"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "local only"]);
    run_git(&fx.repo, &["checkout", "main"]);
    let local = run_git(&fx.repo, &["rev-parse", "release"]);

    let first = fx
        .ws
        .create(&fx.task("Ahead").id, Some("release"), None)
        .unwrap();
    assert_eq!(head(&first), local.trim());

    fs::remove_dir_all(&origin).unwrap();
    let second = fx
        .ws
        .create(&fx.task("Offline").id, Some("release"), None)
        .unwrap();
    assert_eq!(head(&second), local.trim());
}

#[test]
fn starts_a_workspace_from_the_branch_it_is_given() {
    let fx = Fx::new();
    run_git(&fx.repo, &["checkout", "-b", "release"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on release"]);
    run_git(&fx.repo, &["checkout", "main"]);
    let task = fx.task("Fix the bug");

    let created = fx.ws.create(&task.id, Some("release"), None).unwrap();

    assert_eq!(subject(&created), "on release");
}

#[test]
fn starts_a_subtask_from_its_parent_branch_unless_a_base_is_given() {
    let fx = Fx::new();
    let parent = fx.with_workspace("Parent");
    let parent_path = PathBuf::from(parent.workspace.clone().unwrap());
    run_git(
        &parent_path,
        &["commit", "--allow-empty", "-m", "on parent"],
    );
    run_git(&fx.repo, &["branch", "release"]);

    let part_a = fx.task_with(json!({ "title": "Part A", "parentId": parent.id }));
    let child = fx.ws.create(&part_a.id, None, None).unwrap();
    let part_b = fx.task_with(json!({ "title": "Part B", "parentId": parent.id }));
    let picked = fx.ws.create(&part_b.id, Some("release"), None).unwrap();

    assert_eq!(subject(&child), "on parent");
    assert_eq!(subject(&picked), "init");
}

#[test]
fn starts_a_subtask_from_the_default_branch_when_its_parent_has_no_branch() {
    let fx = Fx::new();
    let parent = fx.task("Parent");
    let part = fx.task_with(json!({ "title": "Part A", "parentId": parent.id }));

    let child = fx.ws.create(&part.id, None, None).unwrap();

    assert_eq!(subject(&child), "init");
}

#[test]
fn refuses_a_branch_that_does_not_exist() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    assert!(is_conflict(&fx.ws.create(&task.id, Some("nope"), None)));
}

// Origin gets a branch `feature/x` with a commit "on feature", by its folder: this repository
// has never fetched it.
fn unfetched_feature(fx: &Fx) {
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["checkout", "-b", "scratch"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on feature"]);
    run_git(
        &fx.repo,
        &["push", path_text(&origin), "scratch:refs/heads/feature/x"],
    );
    run_git(&fx.repo, &["checkout", "main"]);
    run_git(&fx.repo, &["branch", "-D", "scratch"]);
}

#[test]
fn lists_the_branches_of_origin_also_the_ones_never_fetched() {
    let fx = Fx::new();
    unfetched_feature(&fx);

    assert_eq!(
        fx.ws.remote_branches(None).unwrap(),
        strings(&["origin/feature/x"])
    );
}

#[test]
fn starts_from_a_branch_of_origin_that_was_never_fetched() {
    let fx = Fx::new();
    unfetched_feature(&fx);

    let created = fx
        .ws
        .create(&fx.task("Remote").id, Some("origin/feature/x"), None)
        .unwrap();

    assert_eq!(subject(&created), "on feature");
    assert!(!git_ok(
        &fx.repo,
        &["rev-parse", "--verify", "refs/heads/feature/x"]
    ));
    let workspace = PathBuf::from(created.workspace.unwrap());
    assert!(!git_ok(&workspace, &["rev-parse", "@{upstream}"]));
}

#[test]
fn refuses_a_branch_that_origin_does_not_have_and_makes_no_worktree() {
    let fx = Fx::new();
    unfetched_feature(&fx);
    let task = fx.task("Typo");

    let made = fx.ws.create(&task.id, Some("origin/nope"), None);
    assert!(is_conflict(&made));
    let worktrees = run_git(&fx.repo, &["worktree", "list", "--porcelain"]);
    assert_eq!(worktrees.matches("worktree ").count(), 1, "{worktrees}");
}
