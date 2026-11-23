// Landlock is Linux only, and these tests build Unix paths.
#![cfg(unix)]

use std::sync::Mutex;

use natamux_git::test_support::TempDir;
use natamux_pty::{Pty, PtyEvents};

use super::*;

struct Dead;

impl Pty for Dead {
    fn start(&mut self, _events: PtyEvents) {}
    fn write(&mut self, _data: &[u8]) {}
    fn resize(&mut self, _cols: u16, _rows: u16) {}
    fn pause(&mut self) {}
    fn resume(&mut self) {}
    fn kill(&mut self, _signal: natamux_pty::Signal) {}
}

/// A spawner that keeps the options it got.
fn recording() -> (Arc<SpawnPty>, Arc<Mutex<Vec<SpawnOptions>>>) {
    let seen: Arc<Mutex<Vec<SpawnOptions>>> = Arc::default();
    let kept = Arc::clone(&seen);
    let spawn = move |options: SpawnOptions| -> io::Result<Box<dyn Pty>> {
        kept.lock().unwrap().push(options);
        Ok(Box::new(Dead))
    };
    (Arc::new(spawn), seen)
}

fn isolation_in(root: &Path, support: Result<u32, String>) -> Isolation {
    Isolation {
        settings: IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
        data_dir: root.join("data"),
        bin_dir: None,
        home: root.join("home"),
        support,
        git_user: Some(("Ann \"A\"".into(), "ann@example.com".into())),
    }
}

fn options(root: &Path, task: &str) -> SpawnOptions {
    let env = [
        ("PATH", "/usr/bin:/bin"),
        ("NATAMUX_TASK_ID", task),
        ("NATAMUX_TOKEN", "T-1.secret"),
        ("SSH_AUTH_SOCK", "/run/agent.sock"),
    ]
    .iter()
    .map(|(name, value)| (name.to_string(), value.to_string()))
    .collect();
    SpawnOptions {
        cols: 80,
        rows: 24,
        cwd: root.join("wt"),
        env,
        isolation: None,
    }
}

#[test]
fn off_passes_the_shell_through_unchanged() {
    let root = TempDir::new("iso-off");
    let (spawn, seen) = recording();
    let wrapped = with_isolation(spawn, None);
    (*wrapped)(options(root.path(), "T-1")).unwrap();
    let seen = seen.lock().unwrap();
    assert!(seen[0].isolation.is_none());
    assert!(seen[0].env.contains_key("SSH_AUTH_SOCK"));
}

#[test]
fn a_machine_without_landlock_starts_no_shell() {
    let root = TempDir::new("iso-none");
    let (spawn, seen) = recording();
    let isolation = isolation_in(root.path(), Err("no Landlock".into()));
    let wrapped = with_isolation(spawn, Some(Arc::new(isolation)));
    let error = (*wrapped)(options(root.path(), "T-1")).err().unwrap();
    assert!(error.to_string().contains("no Landlock"), "{error}");
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn an_isolated_shell_gets_a_plan_a_mark_a_private_tmp_and_its_own_git_config() {
    let root = TempDir::new("iso-on");
    std::fs::create_dir_all(root.path().join("wt")).unwrap();
    let (spawn, seen) = recording();
    let wrapped = with_isolation(spawn, Some(Arc::new(isolation_in(root.path(), Ok(4)))));
    (*wrapped)(options(root.path(), "T-1")).unwrap();
    let seen = seen.lock().unwrap();
    let options = &seen[0];
    let plan = options.isolation.as_ref().unwrap();
    let task_dir = root.path().join("data/sandbox/T-1");
    assert_eq!(
        options.env["TMPDIR"],
        task_dir.join("tmp").to_string_lossy()
    );
    assert!(!options.env.contains_key("SSH_AUTH_SOCK"));
    assert_eq!(options.env["NATAMUX_TOKEN"], "T-1.secret");
    let mark = format!("{SANDBOX_MARK}={}", options.env[SANDBOX_MARK]);
    assert_eq!(plan.sweep_mark.as_deref(), Some(mark.as_str()));
    let config = std::fs::read_to_string(&options.env["GIT_CONFIG_GLOBAL"]).unwrap();
    assert!(config.contains("helper =\n"), "{config}");
    assert!(config.contains("name = \"Ann \\\"A\\\"\""), "{config}");
    assert!(plan.read_write.contains(&root.path().join("wt")));
    assert!(!plan.read_only.contains(&root.path().join("data")));
}

#[test]
fn a_shell_without_a_task_id_is_refused() {
    let root = TempDir::new("iso-id");
    let (spawn, seen) = recording();
    let wrapped = with_isolation(spawn, Some(Arc::new(isolation_in(root.path(), Ok(4)))));
    assert!((*wrapped)(options(root.path(), "../x")).is_err());
    assert!(seen.lock().unwrap().is_empty());
}

#[test]
fn only_claude_code_may_start_and_only_where_isolation_runs() {
    let root = TempDir::new("iso-agent");
    let ready = isolation_in(root.path(), Ok(4));
    assert!(ready.allows_agent("claude-code").is_ok());
    assert!(ready.allows_agent("codex").is_err());
    let none = isolation_in(root.path(), Err("no Landlock".into()));
    assert!(none.allows_agent("claude-code").is_err());
}

#[test]
fn the_view_tells_mode_backend_and_why() {
    let off = isolation_body(None);
    assert_eq!(off.mode, "off");
    assert!(!off.ok);
    let root = TempDir::new("iso-view");
    let none = isolation_in(root.path(), Err("no Landlock".into())).view();
    assert_eq!((none.mode.as_str(), none.ok), ("required", false));
    assert_eq!(none.reason.as_deref(), Some("no Landlock"));
    let ready = isolation_in(root.path(), Ok(5)).view();
    assert_eq!((ready.ok, ready.abi), (true, Some(5)));
}
