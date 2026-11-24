//! T-0373 Part 7 (F1): what an isolated task shell keeps of its environment. Credentials and
//! trace switches never reach it; without isolation the environment is left as it is.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::Path;
use std::sync::{Arc, Mutex};

use natamux_core::security::IsolationSettings;
use natamux_daemon::terminals::isolation::{Isolation, with_isolation};
use natamux_daemon::terminals::session_lifecycle::SpawnPty;
use natamux_git::test_support::TempDir;
use natamux_pty::{Env, Pty, PtyEvents, Signal, SpawnOptions};

const LEAKS: [&str; 5] = [
    "SSH_AUTH_SOCK",
    "GH_TOKEN",
    "GITHUB_TOKEN",
    "AWS_SECRET_ACCESS_KEY",
    "NATAMUX_INPUT_TRACE",
];

struct Dead;

impl Pty for Dead {
    fn start(&mut self, _events: PtyEvents) {}
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, _signal: Signal) {}
}

fn env_seen(root: &Path, isolation: Option<Arc<Isolation>>) -> Env {
    let seen: Arc<Mutex<Option<Env>>> = Arc::default();
    let kept = Arc::clone(&seen);
    let spawn = move |options: SpawnOptions| -> std::io::Result<Box<dyn Pty>> {
        *kept.lock().unwrap() = Some(options.env);
        Ok(Box::new(Dead))
    };
    let spawn: Arc<SpawnPty> = Arc::new(spawn);
    let mut env: Env = LEAKS
        .iter()
        .map(|name| (name.to_string(), "FAKE-SECRET".to_string()))
        .collect();
    for (name, value) in [
        ("PATH", "/usr/bin:/bin"),
        ("NATAMUX_TASK_ID", "T-1"),
        ("NATAMUX_TOKEN", "T-1.task-token"),
        ("ANTHROPIC_API_KEY", "model-key"),
    ] {
        env.insert(name.into(), value.into());
    }
    let wrapped = with_isolation(spawn, isolation);
    (*wrapped)(SpawnOptions {
        cols: 80,
        rows: 24,
        cwd: root.to_path_buf(),
        env,
        isolation: None,
    })
    .unwrap();
    let got = seen.lock().unwrap().clone();
    got.unwrap()
}

#[test]
fn an_isolated_shell_has_the_task_token_and_no_credential() {
    let root = TempDir::new("sandbox-env");
    let isolation = Isolation {
        settings: IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
        data_dir: root.path().join("data"),
        bin_dir: None,
        home: root.path().join("home"),
        support: Ok(4),
        git_user: None,
    };
    let env = env_seen(root.path(), Some(Arc::new(isolation)));
    assert_eq!(env["NATAMUX_TOKEN"], "T-1.task-token");
    assert_eq!(env["ANTHROPIC_API_KEY"], "model-key");
    for name in LEAKS {
        assert!(!env.contains_key(name), "{name} reached the shell");
    }
    assert!(!env.values().any(|value| value == "FAKE-SECRET"));
}

#[test]
fn without_isolation_the_environment_is_unchanged() {
    let root = TempDir::new("sandbox-env-off");
    let env = env_seen(root.path(), None);
    for name in LEAKS {
        assert_eq!(env[name], "FAKE-SECRET");
    }
}

/// T-0375 (R7-9): a name in both `stripExtra` and `envAllow` stays out of the isolated
/// commit-draft agent: its env starts from the one the runner built, not the daemon's.
#[cfg(target_os = "linux")]
#[test]
fn a_stripped_name_stays_out_of_the_isolated_commit_agent() {
    use std::os::unix::fs::PermissionsExt;

    use natamux_core::security::{NetworkSettings, force_network};
    use natamux_daemon::workspaces::agent_once::run_agent_once_with;
    use natamux_git::commit_drafter::AgentRun;

    // Cargo sets it in every test process, so it stands in for a token the daemon has.
    const NAME: &str = "CARGO_PKG_NAME";
    assert!(std::env::var(NAME).is_ok());
    let root = TempDir::new("sandbox-env-agent");
    let worktree = root.path().join("T-1");
    std::fs::create_dir_all(&worktree).unwrap();
    let fake = worktree.join("claude");
    std::fs::write(&fake, "#!/bin/sh\nenv; echo DRAFT-DONE\n").unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let isolation = Isolation::new(
        IsolationSettings {
            required: true,
            env_allow: vec![NAME.into()],
            ..IsolationSettings::default()
        },
        &root.path().join("data"),
        None,
        &root.path().join("home"),
    );
    let _strip = force_network(NetworkSettings {
        strip: true,
        strip_extra: vec![NAME.into()],
        ..NetworkSettings::defaults(false)
    });
    let draft = run_agent_once_with(Some(Arc::new(isolation)));
    let out = (*draft)(AgentRun {
        agent: "claude-code".into(),
        program: fake.display().to_string(),
        prompt: "Write a message".into(),
        cwd: worktree,
        timeout_ms: 10_000,
    })
    .unwrap();
    assert!(out.contains("DRAFT-DONE"), "{out}");
    assert!(!out.contains(&format!("{NAME}=")), "{out}");
}

/// T-0375 (R7-2): a start makes the missing folders Claude Code writes, never its files, and
/// never through a link.
#[cfg(unix)]
#[test]
fn a_start_makes_the_missing_claude_folders_only() {
    let root = TempDir::new("sandbox-env-claude");
    let claude = root.path().join("home/.claude");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(&claude).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, claude.join("todos")).unwrap();
    let isolation = Isolation {
        settings: IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
        data_dir: root.path().join("data"),
        bin_dir: None,
        home: root.path().join("home"),
        support: Ok(4),
        git_user: None,
    };
    env_seen(root.path(), Some(Arc::new(isolation)));
    assert!(claude.join("projects").is_dir());
    assert!(claude.join("file-history").is_dir());
    assert!(claude.join("todos").is_symlink());
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    assert!(!claude.join(".credentials.json").exists());
    assert!(!claude.join("history.jsonl").exists());
}
