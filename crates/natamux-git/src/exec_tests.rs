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
fn reads_the_branch_of_a_linked_worktree_from_its_files_without_git() {
    let (root, repo) = test_repo();
    let worktree = root.path().join("T-0001");
    let path = worktree.to_string_lossy().into_owned();
    run_git(&repo, &["worktree", "add", "-q", "-b", "t-1", &path]);

    let before = thread_spawn_count();
    assert_eq!(current_branch(&worktree).as_deref(), Some("t-1"));
    assert_eq!(thread_spawn_count() - before, 0);

    run_git(&worktree, &["checkout", "-q", "--detach"]);
    let before = thread_spawn_count();
    assert_eq!(current_branch(&worktree), None);
    assert_eq!(thread_spawn_count() - before, 0);
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

#[test]
fn a_git_error_never_shows_the_token_of_a_remote_url() {
    let (_root, repo) = test_repo();
    let url = "https://user:FAKE-TOKEN@host.test/x.git";
    // Git names the pathspec it did not find, URL and all.
    let error = git(&repo, &["checkout", url], None, &[]).unwrap_err();
    assert!(error.0.starts_with("git checkout failed: "), "{error}");
    assert!(error.0.contains("https://host.test/x.git"), "{error}");
    assert!(!error.0.contains("FAKE-TOKEN"), "{error}");
}

fn linked_worktree() -> (crate::test_support::TempDir, PathBuf, PathBuf) {
    let (root, repo) = test_repo();
    let worktree = root.path().join("wt");
    let path = worktree.to_string_lossy().into_owned();
    run_git(&repo, &["worktree", "add", "-q", "-b", "t-1", &path]);
    (root, repo, worktree)
}

#[test]
fn linked_git_dirs_gives_the_real_folders_of_a_linked_worktree_only() {
    let (_root, repo, worktree) = linked_worktree();
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    assert_eq!(
        linked_git_dirs(&worktree),
        Some((common.join("worktrees/wt"), common))
    );
    assert_eq!(linked_git_dirs(&repo), None);
}

#[test]
fn linked_git_dirs_refuses_a_rewritten_git_file_or_commondir() {
    // T-0373 Part 7: the agent writes both files, so they must not choose a granted folder.
    let (root, repo, worktree) = linked_worktree();
    let home = root.path().join("home");
    let fake = root.path().join("fake");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("commondir"), home.to_string_lossy().as_bytes()).unwrap();
    let back = worktree.join(".git");
    std::fs::write(fake.join("gitdir"), back.to_string_lossy().as_bytes()).unwrap();
    let real_git = std::fs::read_to_string(&back).unwrap();
    std::fs::write(&back, format!("gitdir: {}\n", fake.display())).unwrap();
    assert_eq!(linked_git_dirs(&worktree), None);

    std::fs::write(&back, real_git).unwrap();
    let commondir = repo.join(".git/worktrees/wt/commondir");
    std::fs::write(&commondir, home.to_string_lossy().as_bytes()).unwrap();
    assert_eq!(linked_git_dirs(&worktree), None);
}

#[test]
fn linked_git_dirs_refuses_a_git_folder_of_another_worktree() {
    let (root, repo, worktree) = linked_worktree();
    let other = root.path().join("other");
    let path = other.to_string_lossy().into_owned();
    run_git(&repo, &["worktree", "add", "-q", "-b", "t-2", &path]);
    let other_dir = repo.join(".git/worktrees/other");
    std::fs::write(
        worktree.join(".git"),
        format!("gitdir: {}\n", other_dir.display()),
    )
    .unwrap();
    assert_eq!(linked_git_dirs(&worktree), None);
}

#[test]
fn a_closed_network_stops_lazy_fetches_and_lfs_downloads_and_open_adds_nothing() {
    // T-0375 R8-3: both reach a remote from commands the backstop does not know.
    use natamux_core::security::{Egress, NetworkSettings, force_network};
    let (_root, repo) = test_repo();
    let closed = [
        ("GIT_NO_LAZY_FETCH".to_string(), "1".to_string()),
        ("GIT_LFS_SKIP_SMUDGE".to_string(), "1".to_string()),
    ];
    let given = [("A".to_string(), "b".to_string())];
    for egress in [Egress::Loopback, Egress::Allowlist] {
        let _forced = force_network(NetworkSettings {
            egress,
            ..NetworkSettings::defaults(false)
        });
        let env = call_env(&repo, &given);
        assert!(closed.iter().all(|each| env.contains(each)), "{egress:?}");
        assert!(env.contains(&given[0]));
    }
    let _forced = force_network(NetworkSettings::defaults(false));
    // Open and online, and isolation off: the call gets the caller's variables only.
    assert_eq!(call_env(&repo, &given), given);
    assert!(tool_git_env(&repo).is_empty());
}

#[test]
fn a_blob_read_fetches_nothing_while_the_network_is_closed() {
    // T-0375 review 3: a diff reads base blobs, which a `blob:none` clone lacks.
    use natamux_core::security::{Egress, NetworkSettings, force_network};
    let (_root, repo) = test_repo();
    let value_of = |key: &str| {
        let command = blob_command(&repo);
        let found = command.get_envs().find(|(name, _)| *name == key);
        found.and_then(|(_, value)| value.map(|value| value.to_string_lossy().into_owned()))
    };
    {
        let _forced = force_network(NetworkSettings {
            egress: Egress::Loopback,
            ..NetworkSettings::defaults(false)
        });
        for key in ["GIT_NO_LAZY_FETCH", "GIT_LFS_SKIP_SMUDGE"] {
            assert_eq!(value_of(key).as_deref(), Some("1"), "{key}");
        }
    }
    let _forced = force_network(NetworkSettings::defaults(false));
    assert_eq!(value_of("GIT_NO_LAZY_FETCH"), None);
}
