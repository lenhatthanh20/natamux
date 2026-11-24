// Landlock is Linux only, and these tests build Unix paths.
#![cfg(unix)]

use std::path::{Path, PathBuf};

use natamux_core::security::IsolationSettings;
use natamux_git::test_support::TempDir;
use natamux_pty::Env;

use super::*;

fn env_of(pairs: &[(&str, &str)]) -> Env {
    pairs
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect()
}

#[test]
fn keeps_the_allow_list_and_drops_credentials() {
    let env = env_of(&[
        ("PATH", "/usr/bin"),
        ("NATAMUX_TOKEN", "T-1.secret"),
        ("LC_ALL", "C.UTF-8"),
        ("CLAUDE_CODE_USE_BEDROCK", "1"),
        ("ANTHROPIC_API_KEY", "model-key"),
        ("SSH_AUTH_SOCK", "/run/agent"),
        ("GH_TOKEN", "FAKE-GH"),
        ("JAVA_HOME", "/opt/jdk"),
    ]);
    let kept = isolated_env(&env, &["JAVA_HOME".to_string()]);
    let names: Vec<&str> = kept.keys().map(String::as_str).collect();
    assert_eq!(
        names,
        [
            "ANTHROPIC_API_KEY",
            "CLAUDE_CODE_USE_BEDROCK",
            "JAVA_HOME",
            "LC_ALL",
            "NATAMUX_TOKEN",
            "PATH"
        ]
    );
}

#[test]
fn path_grants_only_the_executable_folder_and_checks_the_real_target() {
    let root = TempDir::new("toolchain");
    let home = root.path().join("home");
    let data = home.join(".local/share/natamux");
    let node = home.join(".nvm/node/bin");
    let local_bin = home.join(".local/bin");
    let home_bin = home.join("bin");
    for dir in [&data, &node, &local_bin, &home_bin] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let path = format!(
        "{}:{}:{}:relative/bin:/gone/bin:{}",
        node.display(),
        local_bin.display(),
        home_bin.display(),
        node.display()
    );
    let dirs = toolchain_dirs(&path, &home, &data);
    assert_eq!(
        dirs,
        [node, local_bin, home_bin].map(|p| std::fs::canonicalize(p).unwrap())
    );
    let alias = root.path().join("alias");
    std::os::unix::fs::symlink(&home, &alias).unwrap();
    assert!(toolchain_dirs(alias.to_str().unwrap(), &home, &data).is_empty());
    assert!(toolchain_dirs("/", &home, &data).is_empty());
    assert!(toolchain_dirs(&home.display().to_string(), &home, &data).is_empty());
}

fn plan_in(root: &Path, worktree: &Path, settings: &IsolationSettings) -> IsolationPlan {
    let home = root.join("home");
    let data = root.join("data");
    let task_dir = data.join("sandbox/T-1");
    let tmp = task_dir.join("tmp");
    for dir in [&home.join(".claude"), &tmp] {
        std::fs::create_dir_all(dir).unwrap();
    }
    plan_of(PlanInput {
        worktree,
        task_dir: &task_dir,
        tmp: &tmp,
        home: &home,
        data_dir: &data,
        bin_dir: None,
        path: None,
        settings,
        sweep_mark: Some("NATAMUX_SANDBOX=x".into()),
    })
}

#[test]
fn grants_the_worktree_tmp_and_claude_but_not_home_or_the_data_folder() {
    let root = TempDir::new("plan");
    let worktree = root.path().join("wt");
    std::fs::create_dir_all(&worktree).unwrap();
    let extra = root.path().join("extra");
    std::fs::create_dir_all(&extra).unwrap();
    let settings = IsolationSettings {
        required: true,
        extra_read_only: vec![extra.clone(), PathBuf::from("/gone/ro")],
        ..IsolationSettings::default()
    };
    let plan = plan_in(root.path(), &worktree, &settings);
    let home = root.path().join("home");
    let data = root.path().join("data");
    assert!(plan.read_write.contains(&worktree));
    assert!(plan.read_write.contains(&data.join("sandbox/T-1/tmp")));
    assert!(plan.read_write.contains(&home.join(".claude")));
    assert!(plan.read_only.contains(&data.join("sandbox/T-1")));
    assert!(plan.read_only.contains(&extra));
    assert!(!plan.read_only.contains(&PathBuf::from("/gone/ro")));
    for path in plan.read_only.iter().chain(&plan.read_write) {
        assert_ne!(path, &home, "home is never granted");
        assert_ne!(path, &data, "the data folder is never granted");
        assert!(
            !path.starts_with("/tmp") || path.starts_with(root.path()),
            "{path:?}"
        );
    }
    assert_eq!(plan.sweep_mark.as_deref(), Some("NATAMUX_SANDBOX=x"));
}

#[test]
fn a_linked_worktree_writes_its_git_folder_objects_and_branches_only() {
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    assert!(plan.read_only.contains(&common));
    assert!(plan.read_write.contains(&common.join("worktrees/wt")));
    assert!(plan.read_write.contains(&common.join("objects")));
    assert!(plan.read_write.contains(&common.join("refs/heads")));
    assert!(plan.read_write.contains(&common.join("logs/refs/heads")));
    assert!(!plan.read_write.contains(&common));
    assert!(!plan.read_write.contains(&common.join("hooks")));
}

/// No grant reaches `home` but `~/.claude`, and no git folder was made in it.
fn assert_home_untouched(plan: &IsolationPlan, home: &Path) {
    for path in plan.read_only.iter().chain(&plan.read_write) {
        assert!(
            !path.starts_with(home) || path.starts_with(home.join(".claude")),
            "{path:?}"
        );
    }
    for made in ["objects", "refs", "logs"] {
        assert!(!home.join(made).exists(), "{made} made in home");
    }
}

#[test]
fn a_rewritten_git_file_cannot_grant_the_home_folder() {
    // T-0373 Part 7: `.git` is a worktree file, so the agent picks what it names.
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let home = root.path().join("home");
    let fake = worktree.join("fake");
    std::fs::create_dir_all(&fake).unwrap();
    std::fs::write(fake.join("commondir"), home.to_string_lossy().as_bytes()).unwrap();
    let dot_git = worktree.join(".git");
    std::fs::write(fake.join("gitdir"), dot_git.to_string_lossy().as_bytes()).unwrap();
    std::fs::write(&dot_git, format!("gitdir: {}\n", fake.display())).unwrap();
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    assert_home_untouched(&plan, &home);
    assert!(!plan.read_write.contains(&fake));
}

#[test]
fn a_rewritten_commondir_cannot_grant_the_home_folder() {
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let home = root.path().join("home");
    std::fs::create_dir_all(&home).unwrap();
    let commondir = repo.join(".git/worktrees/wt/commondir");
    std::fs::write(&commondir, home.to_string_lossy().as_bytes()).unwrap();
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    assert_home_untouched(&plan, &home);
}

#[cfg(unix)]
#[test]
fn a_fake_shared_folder_in_tmp_gets_no_grant() {
    // T-0373 Part 7: the agent writes tmp, so it can build a whole shared folder there whose
    // `refs/heads` links to a folder it must not write.
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let secret = root.path().join("secret");
    let fake = root.path().join("data/sandbox/T-1/tmp/fake");
    let git_dir = fake.join("worktrees/wt");
    for dir in [&secret, &git_dir, &fake.join("objects"), &fake.join("refs")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::os::unix::fs::symlink(&secret, fake.join("refs/heads")).unwrap();
    std::fs::write(fake.join("HEAD"), "ref: refs/heads/t-1\n").unwrap();
    std::fs::write(git_dir.join("commondir"), "../..\n").unwrap();
    let dot_git = worktree.join(".git");
    std::fs::write(git_dir.join("gitdir"), dot_git.to_string_lossy().as_bytes()).unwrap();
    std::fs::write(&dot_git, format!("gitdir: {}\n", git_dir.display())).unwrap();
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    let secret = std::fs::canonicalize(&secret).unwrap();
    for path in plan.read_only.iter().chain(&plan.read_write) {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        assert!(!real.starts_with(&secret), "{path:?}");
        assert!(!path.starts_with(&fake), "{path:?}");
    }
}

#[cfg(unix)]
#[test]
fn a_link_inside_the_shared_folder_gets_no_grant() {
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let secret = root.path().join("secret");
    std::fs::create_dir_all(&secret).unwrap();
    let logs = repo.join(".git/logs/refs/heads");
    if logs.exists() {
        std::fs::rename(&logs, root.path().join("old-logs")).unwrap();
    }
    std::fs::create_dir_all(logs.parent().unwrap()).unwrap();
    std::os::unix::fs::symlink(&secret, &logs).unwrap();
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    let secret = std::fs::canonicalize(&secret).unwrap();
    for path in &plan.read_write {
        let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        assert!(!real.starts_with(&secret), "{path:?}");
    }
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    assert!(!plan.read_write.contains(&common.join("worktrees/wt")));
}
