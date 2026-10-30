//! The branch of a new workspace: named by the caller or the project's `git.branchName`, and
//! checked out when it exists here or on origin.

use std::fs;

use super::*;

const CHECKED_OUT: &str = "Branch feature-x is checked out in ";

fn subject(task: &Task) -> String {
    let workspace = PathBuf::from(task.workspace.clone().unwrap());
    run_git(&workspace, &["log", "-1", "--format=%s"])
        .trim()
        .to_string()
}

fn upstream_of(fx: &Fx, branch: &str) -> String {
    let upstream = format!("{branch}@{{upstream}}");
    run_git(&fx.repo, &["rev-parse", "--abbrev-ref", &upstream])
        .trim()
        .to_string()
}

fn has_upstream(fx: &Fx, branch: &str) -> bool {
    let upstream = format!("{branch}@{{upstream}}");
    git_ok(&fx.repo, &["rev-parse", "--verify", &upstream])
}

fn set_pattern(fx: &Fx, pattern: &str) {
    let text = json!({ "git": { "branchName": pattern } }).to_string();
    fs::write(fx.repo.join(".natamux.json"), text).unwrap();
}

// A local branch `feature-x` with a commit "on feature", not checked out anywhere.
fn local_feature(fx: &Fx) {
    run_git(&fx.repo, &["checkout", "-b", "feature-x"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on feature"]);
    run_git(&fx.repo, &["checkout", "main"]);
}

// Origin gets `feature-x` with a commit "on origin", by its folder: this repository never
// fetched it. Gives the commit.
fn origin_feature(fx: &Fx) -> String {
    let origin = bare_origin(fx.root.path());
    run_git(&fx.repo, &["checkout", "-b", "scratch"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on origin"]);
    let commit = run_git(&fx.repo, &["rev-parse", "HEAD"]);
    run_git(
        &fx.repo,
        &["push", path_text(&origin), "scratch:refs/heads/feature-x"],
    );
    run_git(&fx.repo, &["checkout", "main"]);
    run_git(&fx.repo, &["branch", "-D", "scratch"]);
    commit.trim().to_string()
}

fn worktree_count(fx: &Fx) -> usize {
    let listed = run_git(&fx.repo, &["worktree", "list", "--porcelain"]);
    listed.matches("worktree ").count()
}

#[test]
fn names_the_branch_after_the_pattern_of_the_project() {
    let fx = Fx::new();
    set_pattern(&fx, "{id}");
    let task = fx.task("ABC-12: Fix it");
    assert_eq!(task.id, "ABC-12");

    let created = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(created.branch.as_deref(), Some("ABC-12"));
    assert!(!has_upstream(&fx, "ABC-12"));
}

#[test]
fn fills_the_slug_of_the_title_into_the_pattern() {
    let fx = Fx::new();
    set_pattern(&fx, "work/{id}-{slug}");
    let task = fx.task("Fix the bug");

    let created = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(created.branch.as_deref(), Some("work/T-0001-fix-the-bug"));
}

#[test]
fn keeps_the_default_name_without_a_pattern() {
    let fx = Fx::new();
    fs::write(fx.repo.join(".natamux.json"), "{}").unwrap();
    let task = fx.task("Fix the bug");

    let created = fx.ws.create(&task.id, None, None).unwrap();

    assert_eq!(created.branch.as_deref(), Some(BRANCH));
}

#[test]
fn a_named_branch_wins_over_the_pattern() {
    let fx = Fx::new();
    set_pattern(&fx, "{id}");
    let task = fx.task("Fix the bug");

    let created = fx.ws.create(&task.id, None, Some("feature-y")).unwrap();

    assert_eq!(created.branch.as_deref(), Some("feature-y"));
}

#[test]
fn the_kept_branch_of_the_task_wins_over_a_named_one() {
    let fx = Fx::new();
    let task = fx.task("Fix the bug");
    fx.ws.create(&task.id, None, None).unwrap();
    fx.ws.remove(&task.id, false).unwrap();

    let again = fx.ws.create(&task.id, None, Some("feature-y")).unwrap();

    assert_eq!(again.branch.as_deref(), Some(BRANCH));
    assert!(!git_ok(&fx.repo, &["rev-parse", "--verify", "feature-y"]));
}

#[test]
fn refuses_a_pattern_that_fills_a_name_git_does_not_take_and_makes_no_worktree() {
    let fx = Fx::new();
    set_pattern(&fx, "bad..{id}");
    let task = fx.task("Fix the bug");

    let made = fx.ws.create(&task.id, None, None);

    assert!(is_conflict(&made), "{made:?}");
    assert_eq!(error_text(made), "Branch name bad..T-0001 is not valid");
    assert_eq!(worktree_count(&fx), 1);
    assert!(fx.tasks.get_task(&task.id).unwrap().workspace.is_none());
}

#[test]
fn checks_out_a_local_branch_with_its_work_and_ignores_the_base() {
    let fx = Fx::new();
    local_feature(&fx);
    run_git(&fx.repo, &["branch", "release"]);
    let task = fx.task("Fix the bug");

    let created = fx
        .ws
        .create(&task.id, Some("release"), Some("feature-x"))
        .unwrap();

    assert_eq!(created.branch.as_deref(), Some("feature-x"));
    assert_eq!(subject(&created), "on feature");
}

#[test]
fn refuses_a_branch_checked_out_in_the_main_repository_and_leaves_nothing() {
    let fx = Fx::new();
    run_git(&fx.repo, &["checkout", "-b", "feature-x"]);
    let task = fx.task("Fix the bug");

    let made = fx.ws.create(&task.id, None, Some("feature-x"));

    assert!(is_conflict(&made), "{made:?}");
    let text = error_text(made);
    assert!(text.starts_with(CHECKED_OUT), "{text}");
    assert!(text.ends_with("repo"), "{text}");
    assert!(fx.tasks.get_task(&task.id).unwrap().workspace.is_none());
    assert!(!fx.worktrees.join(&task.id).exists());
    assert_eq!(worktree_count(&fx), 1);
}

#[test]
fn refuses_a_branch_checked_out_in_another_worktree_and_leaves_nothing() {
    let fx = Fx::new();
    let first = fx.task("First");
    let first = fx.ws.create(&first.id, None, Some("feature-x")).unwrap();
    let task = fx.task("Second");

    let made = fx.ws.create(&task.id, None, Some("feature-x"));

    assert!(is_conflict(&made), "{made:?}");
    let text = error_text(made);
    assert!(text.starts_with(CHECKED_OUT), "{text}");
    assert!(text.ends_with(first.id.as_str()), "{text}");
    assert!(fx.tasks.get_task(&task.id).unwrap().workspace.is_none());
    assert!(!fx.worktrees.join(&task.id).exists());
    assert_eq!(worktree_count(&fx), 2);
}

#[test]
fn makes_a_local_branch_that_follows_a_branch_only_on_origin() {
    let fx = Fx::new();
    let commit = origin_feature(&fx);
    run_git(&fx.repo, &["branch", "release"]);
    let task = fx.task("Fix the bug");

    let created = fx
        .ws
        .create(&task.id, Some("release"), Some("feature-x"))
        .unwrap();

    assert_eq!(created.branch.as_deref(), Some("feature-x"));
    assert_eq!(upstream_of(&fx, "feature-x"), "origin/feature-x");
    let workspace = PathBuf::from(created.workspace.unwrap());
    let head = run_git(&workspace, &["rev-parse", "HEAD"]);
    assert_eq!(head.trim(), commit);
}

#[test]
fn makes_a_new_branch_from_the_base_for_a_name_that_is_nowhere() {
    let fx = Fx::new();
    bare_origin(fx.root.path());
    run_git(&fx.repo, &["checkout", "-b", "release"]);
    run_git(&fx.repo, &["commit", "--allow-empty", "-m", "on release"]);
    run_git(&fx.repo, &["checkout", "main"]);
    let task = fx.task("Fix the bug");

    let created = fx
        .ws
        .create(&task.id, Some("release"), Some("feature-x"))
        .unwrap();

    assert_eq!(created.branch.as_deref(), Some("feature-x"));
    assert_eq!(subject(&created), "on release");
    assert!(!has_upstream(&fx, "feature-x"));
}
