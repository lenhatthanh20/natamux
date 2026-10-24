use super::*;
use crate::test_support::{bare_origin, run_git, test_repo};
use crate::worktrees::{add_worktree, remove_worktree};
use std::path::PathBuf;

const BRANCH: &str = "natamux/t-0001-example";

fn worktree_path(root: &Path) -> PathBuf {
    root.join("worktrees").join("T-0001")
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    !run_git(repo, &["branch", "--list", branch])
        .trim()
        .is_empty()
}

#[test]
fn deletes_a_branch_that_is_merged() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, None, false).unwrap();
    remove_worktree(&repo, &path, false).unwrap();

    assert!(delete_merged_branch(&repo, BRANCH));
    assert!(!branch_exists(&repo, BRANCH));
}

#[test]
fn keeps_a_branch_that_has_commits_that_are_not_merged() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, None, false).unwrap();
    run_git(&path, &["commit", "--allow-empty", "-m", "work"]);
    remove_worktree(&repo, &path, false).unwrap();

    assert!(!delete_merged_branch(&repo, BRANCH));
    assert!(branch_exists(&repo, BRANCH));
}

#[test]
fn returns_false_for_a_branch_that_does_not_exist() {
    let (_root, repo) = test_repo();
    assert!(!delete_merged_branch(&repo, "natamux/nope"));
}

// A branch with one commit of work, and an origin to push it to.
fn branch_with_work(root: &Path, repo: &Path) -> PathBuf {
    bare_origin(root);
    let path = worktree_path(root);
    add_worktree(repo, &path, BRANCH, None, false).unwrap();
    run_git(&path, &["commit", "--allow-empty", "-m", "work"]);
    path
}

#[test]
fn deletes_a_branch_that_is_not_merged_here_when_origin_has_all_of_it() {
    let (root, repo) = test_repo();
    let path = branch_with_work(root.path(), &repo);
    run_git(&path, &["push", "origin", BRANCH]);
    remove_worktree(&repo, &path, false).unwrap();

    assert!(delete_pushed_branch(&repo, BRANCH));
    assert!(!branch_exists(&repo, BRANCH));
}

#[test]
fn keeps_a_branch_with_a_commit_that_origin_does_not_have() {
    let (root, repo) = test_repo();
    let path = branch_with_work(root.path(), &repo);
    run_git(&path, &["push", "origin", BRANCH]);
    run_git(&path, &["commit", "--allow-empty", "-m", "more"]);
    remove_worktree(&repo, &path, false).unwrap();

    assert!(!delete_pushed_branch(&repo, BRANCH));
    assert!(branch_exists(&repo, BRANCH));
}

#[test]
fn keeps_a_branch_that_origin_does_not_have() {
    let (root, repo) = test_repo();
    let path = branch_with_work(root.path(), &repo);
    remove_worktree(&repo, &path, false).unwrap();

    assert!(!delete_pushed_branch(&repo, BRANCH));
    assert!(branch_exists(&repo, BRANCH));
}

#[test]
fn names_main_the_default_branch_while_the_checkout_is_on_another_one() {
    let (_root, repo) = test_repo();
    run_git(&repo, &["checkout", "-b", "feature"]);
    let list = list_branches(&repo).unwrap();
    assert_eq!(
        list,
        BranchList {
            branches: vec!["feature".to_string(), "main".to_string()],
            current: Some("feature".to_string()),
            default_branch: Some("main".to_string()),
        }
    );
    assert_eq!(
        branch_list_json(&list).to_string(),
        r#"{"branches":["feature","main"],"current":"feature","defaultBranch":"main"}"#
    );
}

#[test]
fn takes_the_default_branch_of_origin_first_when_it_is_a_local_branch() {
    let (root, repo) = test_repo();
    bare_origin(root.path());
    run_git(&repo, &["branch", "trunk"]);
    run_git(&repo, &["push", "origin", "main", "trunk"]);
    run_git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/trunk",
        ],
    );
    assert_eq!(
        list_branches(&repo).unwrap().default_branch.as_deref(),
        Some("trunk")
    );

    // A default branch of origin with no local copy is passed over.
    run_git(
        &repo,
        &[
            "symbolic-ref",
            "refs/remotes/origin/HEAD",
            "refs/remotes/origin/develop",
        ],
    );
    assert_eq!(
        list_branches(&repo).unwrap().default_branch.as_deref(),
        Some("main")
    );
}

#[test]
fn falls_back_to_master_then_the_current_branch_then_none() {
    let (_root, repo) = test_repo();
    run_git(&repo, &["branch", "-m", "main", "master"]);
    run_git(&repo, &["branch", "other"]);
    assert_eq!(
        list_branches(&repo).unwrap().default_branch.as_deref(),
        Some("master")
    );

    run_git(&repo, &["branch", "-m", "master", "trunk"]);
    assert_eq!(
        list_branches(&repo).unwrap().default_branch.as_deref(),
        Some("trunk")
    );

    run_git(&repo, &["checkout", "--detach"]);
    assert_eq!(list_branches(&repo).unwrap().default_branch, None);
}
