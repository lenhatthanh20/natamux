//! T-0373 Part 7 (F1): two tasks in linked worktrees of one repository, isolated with Landlock on
//! a real kernel: a terminal and a stage check command of task A build and commit in A, and
//! cannot read B, the fake home or the daemon token. The shared git folder stays read only apart
//! from objects and branch refs. The commit-draft agent and every other agent are refused unless
//! they are Claude Code.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_core::security::IsolationSettings;
use natamux_daemon::stages::isolated_command::isolated_run_command;
use natamux_daemon::terminals::isolation::{Isolation, with_isolation};
use natamux_daemon::workspaces::agent_once::run_agent_once_with;
use natamux_git::commit_drafter::AgentRun;
use natamux_git::test_support::{TempDir, run_git, test_repo};
use natamux_pty::{Env, PtyEvents, Signal, SpawnOptions, spawn_shell};

const WAIT: Duration = Duration::from_secs(20);

struct Setup {
    _root: TempDir,
    repo: PathBuf,
    a: PathBuf,
    b: PathBuf,
    home: PathBuf,
    data: PathBuf,
    isolation: Arc<Isolation>,
}

fn setup() -> Setup {
    let (root, repo) = test_repo();
    let a = root.path().join("worktrees/T-1");
    let b = root.path().join("worktrees/T-2");
    for (dir, branch) in [(&a, "natamux/t-1"), (&b, "natamux/t-2")] {
        let target = dir.to_string_lossy().into_owned();
        run_git(&repo, &["worktree", "add", "-b", branch, &target]);
    }
    std::fs::write(b.join("sentinel"), "B-SECRET").unwrap();
    let home = root.path().join("home");
    let data = root.path().join("data");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(&data).unwrap();
    std::fs::write(home.join("sentinel"), "HOME-SECRET").unwrap();
    std::fs::write(data.join("daemon-token"), "DAEMON-SECRET").unwrap();
    let mut isolation = Isolation::new(
        IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
        &data,
        None,
        &home,
    );
    isolation.git_user = Some(("Test".into(), "test@example.com".into()));
    assert!(isolation.support.is_ok(), "{:?}", isolation.support);
    Setup {
        _root: root,
        repo,
        a,
        b,
        home,
        data,
        isolation: Arc::new(isolation),
    }
}

fn run_in(setup: &Setup, line: &str, worktree: &Path) -> String {
    let run = isolated_run_command(Arc::clone(&setup.isolation));
    (*run)(line, worktree).output
}

#[test]
fn a_check_command_commits_in_its_worktree_and_reads_nothing_else() {
    let setup = setup();
    let out = run_in(
        &setup,
        "echo built > out.txt && git add out.txt && git commit -q -m build && git log --oneline -1",
        &setup.a,
    );
    assert!(out.contains("build"), "{out}");
    let log = run_git(&setup.a, &["log", "--oneline", "-1"]);
    assert!(log.contains("build"), "{log}");
    for secret in [
        setup.b.join("sentinel"),
        setup.home.join("sentinel"),
        setup.data.join("daemon-token"),
    ] {
        let out = run_in(&setup, &format!("cat '{}'", secret.display()), &setup.a);
        assert!(out.contains("Permission denied"), "{secret:?}: {out}");
        assert!(!out.contains("SECRET"), "{out}");
    }
}

#[test]
fn a_linked_worktree_makes_branches_but_cannot_change_the_shared_config() {
    let setup = setup();
    let out = run_in(&setup, "git branch side && echo BRANCHED", &setup.a);
    assert!(out.contains("BRANCHED"), "{out}");
    let common = setup.repo.join(".git");
    for target in [
        common.join("config"),
        common.join("hooks/pre-commit"),
        common.join("info/exclude"),
        common.join("worktrees/T-2/HEAD"),
    ] {
        let before = std::fs::read(&target).ok();
        let out = run_in(
            &setup,
            &format!("echo evil >> '{}'", target.display()),
            &setup.a,
        );
        assert!(out.contains("Permission denied"), "{target:?}: {out}");
        assert_eq!(std::fs::read(&target).ok(), before, "{target:?} changed");
    }
}

#[test]
fn a_terminal_of_task_a_works_in_a_and_cannot_read_b() {
    let setup = setup();
    let mut env = Env::new();
    for (name, value) in [
        ("PATH", "/usr/bin:/bin"),
        ("SHELL", "/bin/sh"),
        ("PS1", "$ "),
        ("NATAMUX_TASK_ID", "T-1"),
        ("NATAMUX_TOKEN", "T-1.task-token"),
        ("GH_TOKEN", "FAKE-GH-TOKEN"),
    ] {
        env.insert(name.into(), value.into());
    }
    env.insert("HOME".into(), setup.home.display().to_string());
    let spawn = with_isolation(Arc::new(spawn_shell), Some(Arc::clone(&setup.isolation)));
    let mut pty = (*spawn)(SpawnOptions {
        cols: 100,
        rows: 30,
        cwd: setup.a.clone(),
        env,
        isolation: None,
    })
    .unwrap();
    let output = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&output);
    let (exited, _exit) = mpsc::channel();
    pty.start(PtyEvents {
        on_data: Box::new(move |data| sink.lock().unwrap().extend(data)),
        on_exit: Box::new(move |code| {
            let _ = exited.send(code);
        }),
    });
    let b = setup.b.join("sentinel").display().to_string();
    pty.write(
        format!(
            "echo x > made && git add made && git commit -q -m t && echo COMMIT-$((40+2)); \
             cat '{b}' || echo DENIED-$((40+2)); echo \"gh=$GH_TOKEN tmp=$TMPDIR\"\n"
        )
        .as_bytes(),
    );
    let text = wait_for(&output, "DENIED-42");
    assert!(text.contains("COMMIT-42"), "{text}");
    assert!(
        !text.contains("B-SECRET") && !text.contains("FAKE-GH-TOKEN"),
        "{text}"
    );
    assert!(text.contains("sandbox/T-1/tmp"), "{text}");
    pty.kill(Signal::Kill);
}

fn wait_for(output: &Mutex<Vec<u8>>, wanted: &str) -> String {
    let start = Instant::now();
    loop {
        let text = String::from_utf8_lossy(&output.lock().unwrap()).into_owned();
        if text.contains(wanted) {
            return text;
        }
        assert!(start.elapsed() < WAIT, "no {wanted:?} in {text:?}");
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn the_commit_draft_agent_runs_isolated_and_only_as_claude_code() {
    let setup = setup();
    let draft = run_agent_once_with(Some(Arc::clone(&setup.isolation)));
    let run = |agent: &str, program: &str| AgentRun {
        agent: agent.into(),
        program: program.into(),
        prompt: "Write a message".into(),
        cwd: setup.a.clone(),
        timeout_ms: 10_000,
    };
    let refused = (*draft)(run("codex", "codex"));
    assert!(refused.unwrap_err().contains("only Claude Code"));
    // A fake `claude` that tries to read B: the sandbox stops it.
    let fake = setup.a.join("claude");
    let b = setup.b.join("sentinel").display().to_string();
    std::fs::write(
        &fake,
        format!("#!/bin/sh\ncat '{b}' 2>&1; echo DRAFT-DONE\n"),
    )
    .unwrap();
    std::fs::set_permissions(&fake, std::fs::Permissions::from_mode(0o755)).unwrap();
    let out = (*draft)(run("claude-code", &fake.display().to_string())).unwrap();
    assert!(
        out.contains("DRAFT-DONE") && !out.contains("B-SECRET"),
        "{out}"
    );
}
