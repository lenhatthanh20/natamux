use super::*;
use crate::test_support::{run_git, test_repo};

const BRANCH: &str = "natamux/t-0001-example";

fn worktree_path(root: &Path) -> PathBuf {
    root.join("worktrees").join("T-0001")
}

fn branch_exists(repo: &Path, branch: &str) -> bool {
    !run_git(repo, &["branch", "--list", branch])
        .trim()
        .is_empty()
}

// git prints paths its own way (forward slashes and long names on Windows): compare real paths.
fn real_paths<S: AsRef<Path>>(paths: &[S]) -> Vec<PathBuf> {
    paths
        .iter()
        .map(|path| path.as_ref().canonicalize().unwrap())
        .collect()
}

#[test]
fn creates_the_folder_and_the_branch() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();

    assert!(path.join(".git").exists());
    assert!(branch_exists(&repo, BRANCH));
}

#[test]
fn fails_with_a_git_error_when_the_branch_already_exists() {
    let (root, repo) = test_repo();
    add_worktree(&repo, &worktree_path(root.path()), BRANCH, WorktreeBranch::New(None)).unwrap();

    let other = root.path().join("worktrees").join("T-0002");
    let error = add_worktree(&repo, &other, BRANCH, WorktreeBranch::New(None)).unwrap_err();
    assert!(error.0.starts_with("git worktree failed: "), "{error}");
}

#[test]
fn refuses_a_branch_or_base_that_starts_with_a_dash() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    let error = add_worktree(&repo, &path, "--detach", WorktreeBranch::New(None)).unwrap_err();
    assert!(error.0.starts_with("git ref refused: "), "{error}");
    let error =
        add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(Some("--orphan"))).unwrap_err();
    assert!(error.0.starts_with("git ref refused: "), "{error}");
    assert!(!path.exists());
    assert!(!branch_exists(&repo, BRANCH));
}

// Node read the env of its mocked `git`; the env of the call is this function's answer.
#[test]
fn checks_out_with_one_worker_per_cpu() {
    let env = add_worktree_env();
    let key = env
        .iter()
        .find(|(_, value)| value == "checkout.workers")
        .map(|(key, _)| key.clone())
        .unwrap();
    let value_key = key.replace("KEY", "VALUE");
    let value = env.iter().find(|(name, _)| *name == value_key);
    assert_eq!(value.map(|(_, value)| value.as_str()), Some("0"));

    // And a worktree is made with it.
    let (root, repo) = test_repo();
    add_worktree(&repo, &worktree_path(root.path()), BRANCH, WorktreeBranch::New(None)).unwrap();
}

#[test]
fn fails_with_a_git_error_when_the_folder_is_not_a_repository() {
    let (root, _repo) = test_repo();
    let path = worktree_path(root.path());
    assert!(add_worktree(root.path(), &path, "natamux/x", WorktreeBranch::New(None)).is_err());
}

#[test]
fn removes_the_folder() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();

    remove_worktree(&repo, &path, false).unwrap();

    assert!(!path.exists());
}

#[test]
fn refuses_to_remove_a_worktree_with_files_that_are_not_committed() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::write(path.join("notes.txt"), "work in progress").unwrap();

    assert!(remove_worktree(&repo, &path, false).is_err());
    assert!(path.join("notes.txt").exists());
}

#[test]
fn clears_the_entry_of_a_worktree_whose_folder_was_deleted() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::remove_dir_all(&path).unwrap();

    remove_worktree(&repo, &path, false).unwrap();

    let listed = real_paths(&list_worktree_paths(&repo).unwrap());
    assert_eq!(listed, real_paths(&[&repo]));
}

#[test]
fn leaves_another_worktree_and_its_folder_alone_when_it_clears_a_stale_entry() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    let other = root.path().join("worktrees").join("T-0002");
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    add_worktree(&repo, &other, "natamux/t-0002-example", WorktreeBranch::New(None)).unwrap();
    std::fs::write(other.join("notes.txt"), "work in progress").unwrap();
    std::fs::remove_dir_all(&path).unwrap();

    remove_worktree(&repo, &path, false).unwrap();

    let listed = real_paths(&list_worktree_paths(&repo).unwrap());
    assert_eq!(listed, real_paths(&[&repo, &other]));
    assert!(other.join("notes.txt").exists());
}

#[test]
fn reads_the_path_of_each_worktree() {
    let output = [
        "worktree /repo",
        "HEAD 1111111111111111111111111111111111111111",
        "branch refs/heads/main",
        "",
        "worktree /data/Application Support/worktrees/T-0001",
        "HEAD 2222222222222222222222222222222222222222",
        "detached",
        "",
        "worktree /data/worktrees/T-0002",
        "HEAD 3333333333333333333333333333333333333333",
        "branch refs/heads/natamux/t-0002-example",
        "prunable gitdir file points to non-existent location",
        "",
    ]
    .join("\n");

    assert_eq!(
        parse_worktree_list(&output),
        vec![
            "/repo",
            "/data/Application Support/worktrees/T-0001",
            "/data/worktrees/T-0002",
        ]
    );
}

#[test]
fn lists_the_main_checkout_and_each_worktree() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();

    let listed = real_paths(&list_worktree_paths(&repo).unwrap());
    assert_eq!(listed, real_paths(&[&repo, &path]));
}

#[test]
fn is_present_for_a_worktree() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();

    assert_eq!(
        worktree_state(&repo, &path).unwrap(),
        WorktreeState::Present
    );
}

#[test]
fn is_present_for_a_worktree_in_a_folder_with_a_space_in_its_name() {
    let (root, repo) = test_repo();
    let spaced = root.path().join("Application Support").join("T-0001");
    add_worktree(&repo, &spaced, BRANCH, WorktreeBranch::New(None)).unwrap();

    assert_eq!(
        worktree_state(&repo, &spaced).unwrap(),
        WorktreeState::Present
    );
}

// Unix only: a symlink to a folder needs a privilege on Windows.
#[cfg(unix)]
#[test]
fn is_present_for_a_worktree_reached_through_a_symlink() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(root.path().join("worktrees"), &link).unwrap();

    let state = worktree_state(&repo, &link.join("T-0001")).unwrap();
    assert_eq!(state, WorktreeState::Present);
}

#[test]
fn is_stale_when_the_folder_was_deleted_but_git_still_lists_it() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::remove_dir_all(&path).unwrap();

    assert_eq!(worktree_state(&repo, &path).unwrap(), WorktreeState::Stale);
}

// Unix only: a symlink to a folder needs a privilege on Windows.
#[cfg(unix)]
#[test]
fn is_stale_through_a_symlink_when_the_folder_was_deleted() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    let link = root.path().join("link");
    std::os::unix::fs::symlink(root.path().join("worktrees"), &link).unwrap();

    let state = worktree_state(&repo, &link.join("T-0001")).unwrap();
    assert_eq!(state, WorktreeState::Stale);
}

#[test]
fn is_stale_for_a_deleted_folder_whose_path_has_a_space() {
    let (root, repo) = test_repo();
    let spaced = root.path().join("Application Support").join("T-0001");
    add_worktree(&repo, &spaced, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::remove_dir_all(&spaced).unwrap();

    assert_eq!(
        worktree_state(&repo, &spaced).unwrap(),
        WorktreeState::Stale
    );
}

#[test]
fn matches_a_path_with_a_trailing_slash() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    let slashed = PathBuf::from(format!("{}/", path.display()));
    assert_eq!(
        worktree_state(&repo, &slashed).unwrap(),
        WorktreeState::Present
    );

    std::fs::remove_dir_all(&path).unwrap();
    assert_eq!(
        worktree_state(&repo, &slashed).unwrap(),
        WorktreeState::Stale
    );
}

#[test]
fn is_missing_when_the_folder_was_deleted_and_pruned() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    std::fs::remove_dir_all(&path).unwrap();
    run_git(&repo, &["worktree", "prune"]);

    assert_eq!(
        worktree_state(&repo, &path).unwrap(),
        WorktreeState::Missing
    );
}

#[test]
fn is_missing_when_the_parent_folder_is_gone_too() {
    let (root, repo) = test_repo();
    let path = root.path().join("nowhere").join("T-0001");
    assert_eq!(
        worktree_state(&repo, &path).unwrap(),
        WorktreeState::Missing
    );
}

#[test]
fn is_not_a_worktree_for_a_folder_git_does_not_know() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    std::fs::create_dir_all(&path).unwrap();

    assert_eq!(
        worktree_state(&repo, &path).unwrap(),
        WorktreeState::NotAWorktree
    );
    assert_eq!(WorktreeState::NotAWorktree.as_str(), "not-a-worktree");
}

#[test]
fn tracks_a_path_only_when_git_lists_a_file_there() {
    let (_root, repo) = test_repo();
    std::fs::create_dir_all(repo.join(".natamux-task")).unwrap();
    std::fs::write(repo.join(".natamux-task/spec.md"), "from the repo").unwrap();
    std::fs::write(repo.join("other.txt"), "x").unwrap();
    assert!(!tracks_path(&repo, ".natamux-task"));
    run_git(&repo, &["add", ".natamux-task/spec.md"]);
    assert!(tracks_path(&repo, ".natamux-task"));
    assert!(!tracks_path(&repo, "other.txt"));
    // Not a repository: false, no panic.
    let bare = crate::test_support::TempDir::new("plain");
    assert!(!tracks_path(bare.path(), ".natamux-task"));
}

#[test]
fn finds_the_worktree_that_has_a_branch_checked_out() {
    let output = [
        "worktree /repo",
        "HEAD 1111111111111111111111111111111111111111",
        "branch refs/heads/main",
        "",
        "worktree /data/worktrees/T-0001",
        "HEAD 2222222222222222222222222222222222222222",
        "branch refs/heads/feature-x",
        "",
        "worktree /data/worktrees/T-0002",
        "HEAD 3333333333333333333333333333333333333333",
        "detached",
        "",
    ]
    .join("\n");
    let of = |branch| parse_worktree_of_branch(&output, branch);
    assert_eq!(of("main").as_deref(), Some("/repo"));
    assert_eq!(of("feature-x").as_deref(), Some("/data/worktrees/T-0001"));
    // A name that only ends like a checked out one is another branch.
    assert_eq!(of("x"), None);
    assert_eq!(of("feature"), None);
}

#[test]
fn tells_the_worktree_of_a_branch_in_a_real_repository() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(None)).unwrap();
    run_git(&repo, &["branch", "free"]);

    let found = worktree_of_branch(&repo, BRANCH).unwrap().unwrap();
    assert_eq!(real_paths(&[found]), real_paths(&[&path]));
    let main = worktree_of_branch(&repo, "main").unwrap().unwrap();
    assert_eq!(real_paths(&[main]), real_paths(&[&repo]));
    assert_eq!(worktree_of_branch(&repo, "free").unwrap(), None);
}

#[test]
fn checks_out_a_branch_that_is_there_with_its_work() {
    let (root, repo) = test_repo();
    run_git(&repo, &["branch", "feature-x"]);
    run_git(&repo, &["commit", "--allow-empty", "-m", "on main only"]);
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, "feature-x", WorktreeBranch::Existing).unwrap();

    assert_eq!(
        run_git(&path, &["rev-parse", "--abbrev-ref", "HEAD"]).trim(),
        "feature-x"
    );
    assert_eq!(
        run_git(&path, &["rev-parse", "HEAD"]),
        run_git(&repo, &["rev-parse", "main~1"])
    );
}

#[test]
fn makes_a_branch_that_follows_a_remote_ref() {
    let (root, repo) = test_repo();
    crate::test_support::bare_origin(root.path());
    run_git(&repo, &["push", "origin", "main:feature-x"]);
    run_git(&repo, &["fetch", "origin"]);
    let path = worktree_path(root.path());
    let how = WorktreeBranch::Track("refs/remotes/origin/feature-x");
    add_worktree(&repo, &path, "feature-x", how).unwrap();

    let upstream = run_git(&repo, &["rev-parse", "--abbrev-ref", "feature-x@{u}"]);
    assert_eq!(upstream.trim(), "origin/feature-x");
    let error = add_worktree(&repo, &path, "y", WorktreeBranch::Track("--orphan")).unwrap_err();
    assert!(error.0.starts_with("git ref refused: "), "{error}");
}

#[test]
fn a_new_branch_follows_nothing() {
    let (root, repo) = test_repo();
    let path = worktree_path(root.path());
    add_worktree(&repo, &path, BRANCH, WorktreeBranch::New(Some("main"))).unwrap();

    let upstream = format!("{BRANCH}@{{upstream}}");
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", &upstream])
        .current_dir(&repo)
        .output()
        .unwrap();
    assert!(!output.status.success());
}
