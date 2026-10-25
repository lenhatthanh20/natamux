//! The free helpers of `stage-runner.test.ts`: git, temp folders, the fake CI, waits.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::time::{Duration, Instant};

use natamux_core::js::parse_iso_millis;
use natamux_stages::ci::{CiCheck, CiResult};
use serde_json::{Value, json};

use crate::support::FakePty;

/// The issue `gh issue view` gives.
pub fn issue() -> Value {
    json!({
        "number": 7,
        "title": "Export tasks as CSV",
        "body": "We need a CSV export.",
        "url": "https://github.com/o/r/issues/7",
        "labels": [{ "name": "feature" }],
        "comments": [{ "author": { "login": "sam" }, "body": "Dates in UTC, please." }],
    })
}

const IDENTITY: [&str; 6] = [
    "-c",
    "user.name=Test",
    "-c",
    "user.email=test@example.com",
    "-c",
    "commit.gpgsign=false",
];

/// `runGit` of `git-repo.fixture.ts`.
pub fn run_git(cwd: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(IDENTITY)
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A folder for one test, removed when the test ends.
pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-stages-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        // macOS's temp folder is a link; git gives the real path. Windows' canonical form
        // (`\\?\C:\…`) is one git does not take.
        if cfg!(windows) {
            TempDir(path)
        } else {
            TempDir(path.canonicalize().unwrap())
        }
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A CI check that waits until the test finishes it (`finishCi`).
#[derive(Default)]
pub struct FakeCi {
    pub calls: Mutex<Vec<(PathBuf, String)>>,
    finish: Mutex<Option<Sender<CiResult>>>,
    // A check of a stopped runner stays unanswered, as a promise that never settles.
    parked: Mutex<Vec<Sender<CiResult>>>,
}

impl CiCheck for FakeCi {
    fn check(&self, worktree: &Path, branch: &str) -> CiResult {
        let (send, receive) = channel();
        if let Some(old) = self.finish.lock().unwrap().replace(send) {
            self.parked.lock().unwrap().push(old);
        }
        self.calls
            .lock()
            .unwrap()
            .push((worktree.to_path_buf(), branch.to_string()));
        receive
            .recv_timeout(Duration::from_secs(60))
            .unwrap_or_else(|_| natamux_stages::ci::ci_error("the test never finished CI"))
    }
}

impl FakeCi {
    /// Ends the last CI check with `result`, once it has started.
    pub fn finish(&self, result: CiResult) {
        wait_for(|| self.finish.lock().unwrap().is_some());
        if let Some(send) = self.finish.lock().unwrap().take() {
            let _ = send.send(result);
        }
    }
}

/// Polls `check` for up to 5 s, as `vi.waitFor`.
pub fn wait_for(check: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !check() {
        assert!(Instant::now() < until, "the condition never held");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// 2026-10-09T09:00:00.000Z, the start of the clock.
pub fn start_millis() -> i64 {
    parse_iso_millis("2026-10-09T09:00:00.000Z").unwrap()
}

/// The agent got the text as a paste, and its own Enter as the next write. Agents such as the
/// Cursor CLI ignore an Enter in the same write as the paste (`expectSubmitted`).
pub fn expect_submitted(pty: &FakePty, start: &str) {
    let opening = format!("\x1b[200~{start}");
    wait_for(|| {
        let written = pty.written();
        written
            .iter()
            .position(|data| data.starts_with(&opening))
            .is_some_and(|at| {
                written[at].ends_with("\x1b[201~")
                    && written.get(at + 1).map(String::as_str) == Some("\r")
            })
    });
}

/// A paste reaches the shell later, once the shell is ready for it (`expectWritten`).
pub fn expect_written(pty: &FakePty, text: &str) {
    wait_for(|| pty.written().concat().contains(text));
}

/// An action that came from this terminal (`{ terminal }`).
pub fn term_input(terminal: &str) -> natamux_daemon::stages::act::StageActionInput {
    natamux_daemon::stages::act::StageActionInput {
        terminal: Some(terminal.into()),
        ..Default::default()
    }
}
