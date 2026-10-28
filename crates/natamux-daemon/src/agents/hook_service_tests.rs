use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU32, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::Duration;

use natamux_store::task_index::TaskIndex;

use super::*;
use crate::tasks::records::WorkspaceChange;
use crate::tasks::service::TaskServiceOptions;

const WAIT: Duration = Duration::from_secs(10);

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn temp_dir() -> TempDir {
    static COUNT: AtomicU32 = AtomicU32::new(0);
    let count = COUNT.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("natamux-hooks-{}-{count}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    TempDir(path)
}

/// A clock the test moves, so each task gets its own `updated_at`.
fn service(dir: &TempDir, time: &Arc<AtomicI64>) -> TaskService {
    let read = Arc::clone(time);
    TaskService::new(TaskServiceOptions {
        data_dir: dir.0.clone(),
        index: TaskIndex::open(&dir.0.join("index.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: Some(Arc::new(move || read.load(Ordering::SeqCst))),
        settings_of: None,
        clock: None,
    })
}

fn new_task(tasks: &TaskService, time: &AtomicI64) -> Task {
    time.fetch_add(1000, Ordering::SeqCst);
    tasks
        .create_task(Some(&json!({ "title": "Fix it" })), &[], true)
        .unwrap()
}

fn place(tasks: &TaskService, time: &AtomicI64, task: &Task, workspace: &str) {
    time.fetch_add(1000, Ordering::SeqCst);
    let change = WorkspaceChange {
        workspace: Some(workspace.to_string()),
        branch: None,
    };
    tasks.record_workspace(&task.id, change).unwrap();
}

fn hooks(tasks: &TaskService, stages: Arc<dyn StageLookup>) -> Arc<HookService> {
    let options = HookOptions {
        bus: None,
        workspace_changed: None,
        session_seen: None,
        signal_seen: None,
        usage_seen: None,
        home: PathBuf::from("/home/me"),
    };
    Arc::new(HookService::new(tasks.clone(), stages, options))
}

fn stop(hooks: &HookService, task_id: &str) -> Result<HookResult, HookError> {
    let raw = json!({ "hook_event_name": "Stop" });
    hooks.handle("claude-code", &raw, Some(task_id), None)
}

/// Counts the hooks inside `lane_for` at once. The one of `held` waits for `release`, after it
/// says on `entered` that it is inside.
struct Probe {
    inside: AtomicUsize,
    most: AtomicUsize,
    pause: Duration,
    held: Option<String>,
    entered: Mutex<Sender<()>>,
    release: Mutex<Receiver<()>>,
}

impl Probe {
    fn new(held: Option<&str>, pause: Duration) -> (Arc<Probe>, Sender<()>, Receiver<()>) {
        let (entered, seen) = channel();
        let (release, wait) = channel();
        let probe = Probe {
            inside: AtomicUsize::new(0),
            most: AtomicUsize::new(0),
            pause,
            held: held.map(str::to_string),
            entered: Mutex::new(entered),
            release: Mutex::new(wait),
        };
        (Arc::new(probe), release, seen)
    }
}

impl StageLookup for Probe {
    fn lane_for(&self, task: &Task, _terminal: Option<&str>) -> Result<Option<Lane>, String> {
        let now = self.inside.fetch_add(1, Ordering::SeqCst) + 1;
        self.most.fetch_max(now, Ordering::SeqCst);
        if self.held.as_deref() == Some(task.id.as_str()) {
            let _ = self.entered.lock().unwrap().send(());
            let _ = self.release.lock().unwrap().recv_timeout(WAIT);
        }
        thread::sleep(self.pause);
        self.inside.fetch_sub(1, Ordering::SeqCst);
        Ok(None)
    }

    fn take_report(&self, _task_id: &str, _terminal: &str) -> bool {
        false
    }
}

#[test]
fn two_hooks_of_one_task_never_interleave() {
    let (dir, time) = (temp_dir(), Arc::new(AtomicI64::new(1_000_000)));
    let tasks = service(&dir, &time);
    let task = new_task(&tasks, &time);
    let (probe, _release, _seen) = Probe::new(None, Duration::from_millis(50));
    let service = hooks(&tasks, probe.clone());

    let runs: Vec<_> = (0..4)
        .map(|_| {
            let (service, id) = (Arc::clone(&service), task.id.clone());
            thread::spawn(move || stop(&service, &id).unwrap())
        })
        .collect();
    for run in runs {
        run.join().unwrap();
    }

    assert_eq!(probe.most.load(Ordering::SeqCst), 1);
}

#[test]
fn hooks_of_two_tasks_run_at_the_same_time() {
    let (dir, time) = (temp_dir(), Arc::new(AtomicI64::new(1_000_000)));
    let tasks = service(&dir, &time);
    let slow = new_task(&tasks, &time);
    let quick = new_task(&tasks, &time);
    let (probe, release, seen) = Probe::new(Some(&slow.id), Duration::ZERO);
    let service = hooks(&tasks, probe.clone());

    let held = {
        let (service, id) = (Arc::clone(&service), slow.id.clone());
        thread::spawn(move || stop(&service, &id).unwrap())
    };
    seen.recv_timeout(WAIT).unwrap();
    // The first hook is stuck inside: this one must still get through.
    let result = stop(&service, &quick.id).unwrap();
    assert_eq!(result.task_id.as_deref(), Some(quick.id.as_str()));
    assert_eq!(probe.inside.load(Ordering::SeqCst), 1);
    release.send(()).unwrap();
    held.join().unwrap();

    assert_eq!(probe.most.load(Ordering::SeqCst), 2);
}

#[test]
fn drops_the_locks_that_no_hook_holds() {
    let (dir, time) = (temp_dir(), Arc::new(AtomicI64::new(1_000_000)));
    let tasks = service(&dir, &time);
    let service = hooks(&tasks, Arc::new(NoStages));

    drop(service.task_lock("T-0001"));
    let kept = service.task_lock("T-0002");
    drop(service.task_lock("T-0003"));

    let locks = service.serial.lock().unwrap();
    assert_eq!(locks.len(), 2);
    assert!(locks.contains_key("T-0002") && locks.contains_key("T-0003"));
    drop(kept);
}

#[test]
fn finds_the_task_that_list_tasks_lists_first() {
    let (dir, time) = (temp_dir(), Arc::new(AtomicI64::new(1_000_000)));
    let tasks = service(&dir, &time);
    let outer = new_task(&tasks, &time);
    let inner = new_task(&tasks, &time);
    let other = new_task(&tasks, &time);
    place(&tasks, &time, &outer, "/w/a");
    place(&tasks, &time, &inner, "/w/a/b");
    place(&tasks, &time, &other, "/w/c");
    let service = hooks(&tasks, Arc::new(NoStages));

    let old_way = |cwd: &str| {
        tasks.list_tasks(None).unwrap().into_iter().find(|task| {
            task.workspace
                .as_deref()
                .is_some_and(|workspace| is_inside(workspace, cwd))
        })
    };
    for round in 0..2 {
        for cwd in ["/w/a/b/src", "/w/a/x", "/w/c", "/w/c/d/e", "/elsewhere"] {
            let found = service.find_task(Some(cwd), None).unwrap();
            assert_eq!(found.map(|t| t.id), old_way(cwd).map(|t| t.id), "{cwd}");
        }
        // Round two: the outer task is the newer one.
        if round == 0 {
            place(&tasks, &time, &outer, "/w/a");
        }
    }
    let nested = service.find_task(Some("/w/a/b/src"), None).unwrap();
    assert_eq!(nested.map(|task| task.id), Some(outer.id));
}
