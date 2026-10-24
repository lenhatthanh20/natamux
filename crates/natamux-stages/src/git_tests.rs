use super::*;
use crate::test_support::{TempDir, run_git, test_repo};

#[test]
fn reads_head_count_and_dirty_files() {
    let (_root, repo) = test_repo();
    let head = current_head(&repo).unwrap();
    assert_eq!(head.len(), 40);
    assert_eq!(commit_count(&repo, "HEAD").unwrap(), 1);
    assert_eq!(commit_count(&repo, &format!("{head}..HEAD")).unwrap(), 0);
    assert!(dirty_files(&repo).unwrap().is_empty());
    std::fs::write(repo.join("a.txt"), "x").unwrap();
    assert_eq!(dirty_files(&repo).unwrap(), vec!["?? a.txt".to_string()]);
    run_git(&repo, &["add", "."]);
    run_git(&repo, &["commit", "-m", "a"]);
    assert_eq!(commit_count(&repo, &format!("{head}..HEAD")).unwrap(), 1);
    assert_eq!(common_dir(&repo).unwrap(), ".git");
}

#[test]
fn has_no_head_outside_a_repository() {
    let dir = TempDir::new("nogit");
    assert_eq!(current_head(dir.path()), None);
    let error = commit_count(dir.path(), "HEAD").unwrap_err();
    assert!(error.0.starts_with("git rev-list failed: "));
}

#[test]
fn does_not_pull_a_branch_without_an_upstream() {
    let (_root, repo) = test_repo();
    assert_eq!(pull_fast_forward(&repo), Ok(false));
}

#[test]
fn pulls_new_commits_of_the_upstream() {
    let (root, repo) = test_repo();
    let remote = root.path().join("origin.git");
    run_git(
        root.path(),
        &["init", "--bare", "-b", "main", remote.to_str().unwrap()],
    );
    run_git(
        &repo,
        &["remote", "add", "origin", remote.to_str().unwrap()],
    );
    run_git(&repo, &["push", "--set-upstream", "origin", "main"]);
    let other = root.path().join("other");
    run_git(
        root.path(),
        &["clone", remote.to_str().unwrap(), other.to_str().unwrap()],
    );
    run_git(&other, &["commit", "--allow-empty", "-m", "more"]);
    run_git(&other, &["push"]);
    assert_eq!(pull_fast_forward(&repo), Ok(true));
    assert_eq!(commit_count(&repo, "HEAD").unwrap(), 2);
}

#[test]
fn adds_git_config_entries_after_the_ones_there() {
    let env = with_git_config(Some("2"), &GITHUB_CREDENTIALS);
    assert_eq!(
        env,
        vec![
            ("GIT_CONFIG_COUNT".to_string(), "3".to_string()),
            (
                "GIT_CONFIG_KEY_2".to_string(),
                "credential.https://github.com.helper".to_string()
            ),
            (
                "GIT_CONFIG_VALUE_2".to_string(),
                "!gh auth git-credential".to_string()
            ),
        ]
    );
    assert_eq!(with_git_config(None, &[])[0].1, "0");
}
