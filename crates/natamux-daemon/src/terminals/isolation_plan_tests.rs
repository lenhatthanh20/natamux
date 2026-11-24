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
fn a_bin_folder_in_home_keeps_only_itself_and_never_reaches_the_data_folder() {
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
    // Inside home a bin folder never gives its parent.
    assert_eq!(dirs, [node, local_bin, home_bin]);
    assert!(toolchain_dirs("/", &home, &data).is_empty());
    assert!(toolchain_dirs(&home.display().to_string(), &home, &data).is_empty());
}

#[test]
fn a_bin_folder_in_home_gives_its_lib_siblings_and_outside_home_its_parent() {
    // T-0375 (R7-6): `~/.cargo` holds `credentials.toml`.
    let root = TempDir::new("toolchain-home");
    let home = root.path().join("home");
    let data = root.path().join("data");
    let cargo = home.join(".cargo");
    let tool = root.path().join("opt/tool");
    for dir in [
        &cargo.join("bin"),
        &cargo.join("lib"),
        &tool.join("bin"),
        &data,
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    let path = format!(
        "{}:{}",
        cargo.join("bin").display(),
        tool.join("bin").display()
    );
    let dirs = toolchain_dirs(&path, &home, &data);
    assert_eq!(dirs, [cargo.join("bin"), cargo.join("lib"), tool]);
    assert!(!dirs.contains(&cargo));
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
    assert!(plan.read_only.contains(&home.join(".claude")));
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
fn only_what_claude_code_writes_while_it_works_is_writable_in_its_folder() {
    // T-0375 (R7-2): hooks, MCP servers or commands planted there would run outside the sandbox.
    let root = TempDir::new("plan-claude");
    let worktree = root.path().join("wt");
    let home = root.path().join("home");
    let claude = home.join(".claude");
    let outside = root.path().join("outside");
    for dir in [
        &worktree,
        &claude.join("projects"),
        &claude.join("commands"),
        &outside,
    ] {
        std::fs::create_dir_all(dir).unwrap();
    }
    for file in [claude.join("settings.json"), home.join(".claude.json")] {
        std::fs::write(file, "{}").unwrap();
    }
    std::os::unix::fs::symlink(&outside, claude.join("todos")).unwrap();
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    assert!(plan.read_write.contains(&claude.join("projects")));
    assert!(plan.read_only.contains(&claude));
    assert!(plan.read_only.contains(&home.join(".claude.json")));
    for kept in [
        claude.join("settings.json"),
        claude.join("commands"),
        home.join(".claude.json"),
        claude.join("todos"),
        outside,
    ] {
        assert!(
            !plan.read_write.iter().any(|path| kept.starts_with(path)),
            "{kept:?} is writable"
        );
    }
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

/// A repo at `fake` that passes `linked_git_dirs`, with the worktree's `.git` pointed at it.
fn fake_repo(fake: &Path, worktree: &Path) {
    let git_dir = fake.join("worktrees/wt");
    for dir in [&git_dir, &fake.join("objects"), &fake.join("refs")] {
        std::fs::create_dir_all(dir).unwrap();
    }
    std::fs::write(fake.join("HEAD"), "ref: refs/heads/t-1\n").unwrap();
    std::fs::write(fake.join("config"), "[http]\n\tsslVerify = false\n").unwrap();
    std::fs::write(git_dir.join("commondir"), "../..\n").unwrap();
    let dot_git = worktree.join(".git");
    std::fs::write(git_dir.join("gitdir"), dot_git.to_string_lossy().as_bytes()).unwrap();
    std::fs::write(&dot_git, format!("gitdir: {}\n", git_dir.display())).unwrap();
}

#[test]
fn a_fake_repo_in_the_git_write_grants_gets_no_grant_and_keeps_the_real_pin() {
    // T-0375 review 1: the agent writes the shared objects and branch refs, so it can build a
    // repo there that passes every check of the `.git` file.
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let settings = IsolationSettings::default();
    plan_in(root.path(), &worktree, &settings);
    let real = natamux_git::exec::linked_git_dirs(&worktree);
    assert!(real.is_some());
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    for fake in [common.join("objects/x"), common.join("refs/heads/x")] {
        fake_repo(&fake, &worktree);
        assert!(
            natamux_git::exec::linked_git_dirs(&worktree).is_some(),
            "{fake:?}"
        );
        let plan = plan_in(root.path(), &worktree, &settings);
        for path in plan.read_only.iter().chain(&plan.read_write) {
            assert!(!path.starts_with(&fake), "{path:?}");
        }
        let (_, pinned) = natamux_core::security::pinned_git_dirs(&worktree).unwrap();
        assert_eq!(pinned, real);
    }
}

#[test]
fn a_fake_repo_there_at_the_daemon_start_is_refused() {
    // T-0375 review 1: no pair was pinned before, so only the check itself can refuse it.
    let (root, repo) = natamux_git::test_support::test_repo();
    let worktree = root.path().join("wt");
    let target = worktree.to_string_lossy().into_owned();
    natamux_git::test_support::run_git(&repo, &["worktree", "add", "-b", "t-1", &target]);
    let common = std::fs::canonicalize(repo.join(".git")).unwrap();
    let fake = common.join("objects/x");
    fake_repo(&fake, &worktree);
    let plan = plan_in(root.path(), &worktree, &IsolationSettings::default());
    for path in plan.read_only.iter().chain(&plan.read_write) {
        assert!(!path.starts_with(&common), "{path:?}");
    }
    let (_, pinned) = natamux_core::security::pinned_git_dirs(&worktree).unwrap();
    assert_eq!(pinned, None);
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
