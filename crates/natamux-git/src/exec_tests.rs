use super::*;
use crate::gh::run_gh;
use crate::test_support::{run_git, test_repo};

#[test]
fn gives_the_branch_of_a_checkout_and_none_when_head_is_detached() {
    let (_root, repo) = test_repo();
    assert_eq!(current_branch(&repo).as_deref(), Some("main"));

    run_git(&repo, &["checkout", "--detach"]);
    assert_eq!(current_branch(&repo), None);
    // A subfolder has no `.git` of its own: git answers.
    std::fs::create_dir(repo.join("sub")).unwrap();
    run_git(&repo, &["checkout", "-q", "main"]);
    assert_eq!(current_branch(&repo.join("sub")).as_deref(), Some("main"));
}

#[test]
fn adds_settings_after_the_ones_that_are_there() {
    let env = with_git_config(Some("1"), &[("a.b", "c")]);
    let has = |key: &str, value: &str| env.contains(&(key.to_string(), value.to_string()));
    assert!(has("GIT_CONFIG_COUNT", "2"));
    assert!(has("GIT_CONFIG_KEY_1", "a.b"));
    assert!(has("GIT_CONFIG_VALUE_1", "c"));
    // The entries that are there stay with the process environment: only the new ones are given.
    assert!(!env.iter().any(|(key, _)| key == "GIT_CONFIG_KEY_0"));
}

#[test]
fn starts_at_zero_without_settings() {
    assert_eq!(
        with_git_config(None, &[("a.b", "c")]),
        vec![
            ("GIT_CONFIG_COUNT".to_string(), "1".to_string()),
            ("GIT_CONFIG_KEY_0".to_string(), "a.b".to_string()),
            ("GIT_CONFIG_VALUE_0".to_string(), "c".to_string()),
        ]
    );
}

// The count is for the whole daemon and tests run in parallel, so the exact numbers are read
// from the counter of this thread; the global one must have grown by at least as much.
#[test]
fn counts_each_git_and_gh_process_also_the_ones_that_fail() {
    let (root, repo) = test_repo();
    let (global, mine) = (spawn_count(), thread_spawn_count());

    // `current_branch` reads `.git/HEAD` without git, so `repository_root` runs git here.
    repository_root(&repo).unwrap();
    repository_root(&repo).unwrap();
    // Not a repository: git exits with an error.
    assert!(repository_root(root.path()).is_err());
    assert_eq!(thread_spawn_count() - mine, 3);

    // A folder that does not exist: the process cannot start.
    let missing = root.path().join("missing");
    for _ in 0..2 {
        let error = run_gh(&missing, &["--version".to_string()]).unwrap_err();
        assert!(error.message.starts_with("GitHub CLI failed: "), "{error}");
    }
    assert_eq!(thread_spawn_count() - mine, 5);
    assert!(spawn_count() - global >= 5);
}

#[test]
fn checked_ref_refuses_empty_names_and_names_that_look_like_options() {
    assert_eq!(checked_ref("main").unwrap(), "main");
    assert_eq!(
        checked_ref("origin/natamux/t-1").unwrap(),
        "origin/natamux/t-1"
    );
    for bad in ["", "-b", "--upload-pack=touch x"] {
        let error = checked_ref(bad).unwrap_err();
        assert!(error.0.starts_with("git ref refused: "), "{error}");
    }
}
