//! `natamux support-bundle` and `natamux purge-data` (T-0373 Part 9).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use natamux_cli::run::{EXIT_FAILED, EXIT_OK, EXIT_USAGE, Env};
use serde_json::json;
use support::*;

fn temp(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("natamux-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn git(dir: &Path, args: &[&str]) {
    let done = Command::new("git").arg("-C").arg(dir).args(args).output();
    assert!(done.unwrap().status.success(), "git {args:?}");
}

/// A data folder with every kind of task data, and one clean worktree.
fn data_folder(name: &str) -> (PathBuf, PathBuf) {
    let data = temp(name);
    for folder in ["tasks", "logs", "sandbox/T-1"] {
        fs::create_dir_all(data.join(folder)).unwrap();
    }
    for file in [
        "tasks/T-1.md",
        "logs/audit.log",
        "index.sqlite",
        "index.sqlite-wal",
        "agent-launch-T-1.sh",
        "stage-command-T-1.sh",
        "repos.json",
        "security.json",
    ] {
        fs::write(data.join(file), "PURGE-MARK").unwrap();
    }
    let worktree = data.join("worktrees/T-1");
    fs::create_dir_all(&worktree).unwrap();
    git(&worktree, &["init", "-q"]);
    (data, worktree)
}

/// Port 0 is never probed, so a daemon on the default port of this machine changes nothing.
fn env_of(data: &Path) -> Env {
    Env::from([
        ("NATAMUX_DATA_DIR", data.to_str().unwrap()),
        ("NATAMUX_PORT", "0"),
    ])
}

#[test]
fn support_bundle_saves_the_answer_to_a_private_file() {
    let dir = temp("bundle");
    let out = dir.join("bundle.json");
    let body = json!({ "daemonVersion": "1.0.36", "tasks": { "done": 2 } });
    let argv = ["support-bundle", "--out", out.to_str().unwrap()];
    let result = run(&argv, move |_| json_reply(body.clone()));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert_eq!(result.lines(), ["GET http://127.0.0.1:4317/support-bundle"]);
    let saved: serde_json::Value = serde_json::from_slice(&fs::read(&out).unwrap()).unwrap();
    assert_eq!(saved["tasks"]["done"], 2);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = fs::metadata(&out).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let other = dir.join("other.json");
    let argv = ["support-bundle", "--out", other.to_str().unwrap()];
    let result = run(&argv, |_| json_status(403, json!({ "error": "forbidden" })));
    assert_eq!(result.code, EXIT_FAILED);
    assert!(!other.exists(), "no file for a refused bundle");
    fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn purge_data_removes_task_data_keeps_worktrees_and_settings_and_runs_twice() {
    let (data, worktree) = data_folder("purge");
    let asked = run_plain(&["purge-data"], env_of(&data));
    assert_eq!(asked.code, EXIT_USAGE, "{}", asked.stderr);
    assert!(
        data.join("tasks/T-1.md").exists(),
        "nothing goes without --yes"
    );

    let result = run_plain(&["purge-data", "--yes"], env_of(&data));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    for gone in [
        "tasks",
        "logs",
        "sandbox",
        "index.sqlite",
        "index.sqlite-wal",
        "agent-launch-T-1.sh",
        "stage-command-T-1.sh",
    ] {
        assert!(!data.join(gone).exists(), "{gone}");
    }
    for kept in ["repos.json", "security.json"] {
        assert!(data.join(kept).exists(), "{kept}");
    }
    assert!(worktree.join(".git").exists(), "the worktree stays");

    let again = run_plain(&["purge-data", "--yes"], env_of(&data));
    assert_eq!(again.code, EXIT_OK, "{}", again.stderr);
    assert_eq!(again.stdout, "Nothing to remove.\n");
    fs::remove_dir_all(&data).unwrap();
}

#[test]
fn purge_data_refuses_a_dirty_worktree_and_a_task_terminal() {
    let (data, worktree) = data_folder("purge-dirty");
    fs::write(worktree.join("work.txt"), "not committed").unwrap();
    let result = run_plain(&["purge-data", "--yes"], env_of(&data));
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("worktrees/T-1"), "{}", result.stderr);
    assert!(data.join("tasks/T-1.md").exists());
    assert!(worktree.join("work.txt").exists());

    fs::remove_file(worktree.join("work.txt")).unwrap();
    let data_text = data.to_str().unwrap();
    let in_task = Env::from([("NATAMUX_DATA_DIR", data_text), ("NATAMUX_TASK_ID", "T-1")]);
    let result = run_plain(&["purge-data", "--yes"], in_task);
    assert_eq!(result.code, EXIT_USAGE);
    assert!(data.join("tasks/T-1.md").exists());
    fs::remove_dir_all(&data).unwrap();
}

#[test]
fn purge_data_refuses_while_a_daemon_answers_on_the_port() {
    let (data, _worktree) = data_folder("purge-daemon");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port().to_string();
    let data_text = data.to_str().unwrap();
    let env = Env::from([
        ("NATAMUX_DATA_DIR", data_text),
        ("NATAMUX_PORT", port.as_str()),
    ]);
    let result = run_plain(&["purge-data", "--yes"], env);
    assert_eq!(result.code, EXIT_FAILED, "{}", result.stderr);
    assert!(
        result.stderr.contains("daemon is running"),
        "{}",
        result.stderr
    );
    assert!(data.join("tasks/T-1.md").exists());

    drop(listener);
    let env = Env::from([
        ("NATAMUX_DATA_DIR", data_text),
        ("NATAMUX_PORT", port.as_str()),
    ]);
    let result = run_plain(&["purge-data", "--yes"], env);
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    fs::remove_dir_all(&data).unwrap();
}
