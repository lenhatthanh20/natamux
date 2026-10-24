use std::process::Command;
use std::sync::atomic::AtomicBool;
use std::sync::mpsc::Receiver;
use std::time::Instant;

use natamux_git::gh::RunGh;

use super::*;

/// The diff stat of one task, as `diff_stats` gives it now.
pub fn stat_of(ws: &WorkspaceService, task_id: &str) -> Option<DiffStat> {
    let stats = ws.diff_stats().unwrap();
    stats
        .iter()
        .find(|(id, _)| id == task_id)
        .map(|(_, stat)| *stat)
}

/// The diff stats of every task, by task id.
pub fn stat_map(ws: &WorkspaceService) -> std::collections::BTreeMap<String, DiffStat> {
    ws.diff_stats().unwrap().into_iter().collect()
}

pub fn stat(files: u64, added: u64, removed: u64) -> DiffStat {
    DiffStat {
        files,
        added,
        removed,
    }
}

/// A path that is a folder or a file, as a list: the names in it, or the text of the file.
pub fn contents_of(path: &Path) -> Vec<String> {
    if path.is_dir() {
        let mut names: Vec<String> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    } else {
        vec![fs::read_to_string(path).unwrap()]
    }
}

/// A folder deleted outside Natamux: `pruned` when git forgot it too, else git still lists it.
pub fn delete_folder(fx: &Base, path: &Path, pruned: bool) {
    let _ = fs::remove_dir_all(path);
    if pruned {
        run_git(&fx.repo, &["worktree", "prune"]);
    }
}

/// Whether a git command succeeds (Node's `expect(() => runGit(..)).toThrow()` is the `false`).
pub fn git_ok(cwd: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap()
        .status
        .success()
}

pub fn path_text(path: &Path) -> &str {
    path.to_str().unwrap()
}

pub fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(ToString::to_string).collect()
}

pub fn is_conflict<T>(result: &Result<T, WorkspaceError>) -> bool {
    matches!(result, Err(WorkspaceError::Conflict(_)))
}

pub fn is_git<T>(result: &Result<T, WorkspaceError>) -> bool {
    matches!(result, Err(WorkspaceError::Git(_)))
}

pub fn is_not_changed<T>(result: &Result<T, WorkspaceError>) -> bool {
    matches!(result, Err(WorkspaceError::NotChanged(_)))
}

/// The text of the error of a call that must fail.
pub fn error_text<T: std::fmt::Debug>(result: Result<T, WorkspaceError>) -> String {
    result.unwrap_err().to_string()
}

/// Waits until `check` is true, for at most 10 s (Node's `vi.waitFor`).
pub fn wait_for(what: &str, check: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(10);
    while !check() {
        assert!(Instant::now() < until, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Holds the first change that shows as a job: its `jobs.changed` listener blocks until `release`.
/// The change keeps its task busy meanwhile, so a second change of the task is refused.
pub struct Gate {
    started: Mutex<Receiver<()>>,
    release: Sender<()>,
}

impl Gate {
    /// Jobs on a bus of their own, with the gate on it. `sent` gets every message, before the gate.
    pub fn jobs(fx: &Base) -> (Arc<Jobs>, Messages, Gate) {
        let bus = EventBus::new();
        let sent: Messages = Arc::default();
        record(&bus, &sent);
        let (started_tx, started) = channel();
        let (release, release_rx) = channel::<()>();
        let (started_tx, release_rx) = (Mutex::new(started_tx), Mutex::new(release_rx));
        let fired = AtomicBool::new(false);
        bus.subscribe(move |message| {
            let DaemonMessage::JobsChanged { jobs } = message else {
                return;
            };
            if jobs.len() == 1 && !fired.swap(true, Ordering::SeqCst) {
                started_tx.lock().unwrap().send(()).unwrap();
                let _ = release_rx.lock().unwrap().recv();
            }
        });
        let jobs = Arc::new(Jobs::new(Some(bus), clock_of(&fx.clock)));
        let gate = Gate {
            started: Mutex::new(started),
            release,
        };
        (jobs, sent, gate)
    }

    /// Waits until the change is running.
    pub fn wait_started(&self) {
        self.started
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(10))
            .expect("the change did not start");
    }

    pub fn release(&self) {
        let _ = self.release.send(());
    }
}

/// An agent run (`run_agent_once`) that ends when the test says so (Node's `heldAgent`).
#[derive(Clone, Default)]
pub struct Held {
    pub started: Arc<AtomicBool>,
    finish: Arc<Mutex<Option<Sender<String>>>>,
}

impl Held {
    pub fn run(&self) -> natamux_daemon::workspaces::service::RunAgentOnce {
        let me = self.clone();
        Arc::new(move |_run| {
            let (done, wait) = channel();
            *me.finish.lock().unwrap() = Some(done);
            me.started.store(true, Ordering::SeqCst);
            wait.recv_timeout(Duration::from_secs(30))
                .map_err(|_| "the held agent was never finished".to_string())
        })
    }

    pub fn wait_started(&self) {
        wait_for("the agent to start", || self.started.load(Ordering::SeqCst));
    }

    pub fn finish(&self, text: &str) {
        let done = self.finish.lock().unwrap().take().expect("no run is held");
        done.send(text.to_string()).unwrap();
    }
}

/// A gh that waits for `release` (Node's `blocked` promise), then answers with a pull request.
pub fn blocked_gh() -> (RunGh, Sender<()>) {
    let (release, wait) = channel::<()>();
    let wait = Mutex::new(wait);
    let gh: RunGh = Arc::new(move |_cwd: &Path, _args: &[String]| {
        let _ = wait.lock().unwrap().recv_timeout(Duration::from_secs(30));
        Ok("https://github.com/o/r/pull/1\n".to_string())
    });
    (gh, release)
}
