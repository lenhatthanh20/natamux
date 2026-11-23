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
fn a_bin_folder_gives_its_parent_unless_that_reaches_home_or_the_data_folder() {
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
    // ~/.local holds the data folder and ~/bin's parent is home: both keep only the bin folder.
    assert_eq!(dirs, [home.join(".nvm/node"), local_bin, home_bin]);
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
