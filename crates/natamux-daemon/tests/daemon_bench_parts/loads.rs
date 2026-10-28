//! The daemon numbers of T-0223 P1 (docs/perf.md): memory after busy shells, the task load at
//! start, and the hook time while many agents work at once. Reports only; no targets here.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Barrier};
use std::time::{Duration, Instant};

use natamux_daemon::agents::hook_service::{HookOptions, HookService, NoStages};
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::terminals::limits::MAX_SESSIONS;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_store::task_files::write_task_file;
use natamux_store::task_index::TaskIndex;
use natamux_store::task_sync::{Clock, TaskSync};
use serde_json::{Value, json};

use super::{COLS, Sink, TASK, emit, open, rss_kib, setup};

/// The report's "30 agents with long output".
pub const BUSY_SHELLS: usize = 30;
const BUSY_BYTES: usize = 20 * 1024 * 1024;
pub const AGENTS_AT_ONCE: usize = 30;
const HOOKS_PER_AGENT: usize = 40;

/// The daemon's RSS in MiB once 30 shells each printed 20 MB, from 30 threads at once.
pub fn busy_shells() -> f64 {
    let (service, sinks) = setup();
    let shells = BUSY_SHELLS.min(MAX_SESSIONS);
    let names: Vec<String> = (1..=shells).map(|index| format!("busy-{index}")).collect();
    for name in &names {
        open(&service, name);
    }
    let line = format!("{}\r\n", "b".repeat(usize::from(COLS) - 2));
    let block = line.repeat(64 * 1024 / line.len()).into_bytes();
    let all: Vec<Sink> = sinks.lock().unwrap().clone();
    let printers: Vec<_> = all
        .into_iter()
        .map(|sink| {
            let block = block.clone();
            std::thread::spawn(move || {
                for _ in 0..BUSY_BYTES / block.len() {
                    emit(&sink, &block);
                }
            })
        })
        .collect();
    for printer in printers {
        printer.join().unwrap();
    }
    for name in &names {
        service.settled(TASK, name);
    }
    let rss = rss_kib().unwrap_or(0);
    service.close_all();
    (rss as f64 / 1024.0 * 10.0).round() / 10.0
}

/// `TaskSync::init` with `count` task files, in ms: cold (a new index, every file read) and
/// warm (the index of the first load, as at a daemon start with no change).
pub fn task_init(count: usize) -> (f64, f64) {
    let dir = temp_dir(&format!("tasks-{count}"));
    for number in 1..=count {
        write_task_file(&dir, &bench_task(number)).unwrap();
    }
    let cold = init_time(&dir, count);
    let warm = init_time(&dir, count);
    std::fs::remove_dir_all(&dir).ok();
    (cold, warm)
}

fn init_time(dir: &Path, count: usize) -> f64 {
    let index = TaskIndex::open(&dir.join("index.sqlite")).unwrap();
    let clock: Clock = Arc::new(|| 0);
    let mut sync = TaskSync::new(dir, index, clock, Box::new(|_| {}));
    let start = Instant::now();
    sync.init().unwrap();
    let took = start.elapsed();
    assert_eq!(sync.tasks.len(), count, "a bench task file did not load");
    fine_ms(took)
}

fn bench_task(number: usize) -> Task {
    Task {
        id: format!("T-{number:04}"),
        title: format!("Bench task {number} with a title of a usual length"),
        description: "What to build, in a few lines.\n\nAnd a second part.".into(),
        state: TaskState::Running,
        priority: "normal".into(),
        labels: vec!["bench".into()],
        agent: Some("claude-code".into()),
        branch: Some(format!("natamux/t-{number:04}-bench")),
        workspace: Some(format!("/work/T-{number:04}")),
        stage: "implement".into(),
        // A task file needs at least one stage to load.
        stages: ["spec", "implement", "review"].map(Value::from).to_vec(),
        created_at: "2026-10-08T09:00:00.000Z".into(),
        updated_at: "2026-10-08T09:30:00.000Z".into(),
        ..Task::default()
    }
}

/// `HookService::handle` while `agents` agents each send hooks from their own thread, all at
/// once: p50 and p95 in ms. The task is found by the hook's working directory.
pub fn hooks_at_once(agents: usize) -> (f64, f64) {
    let dir = temp_dir("hooks");
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: dir.clone(),
        index: TaskIndex::open(&dir.join("index.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock: None,
    });
    let mut workspaces = Vec::new();
    for agent in 0..agents {
        let title = json!({ "title": format!("Bench agent {agent}") });
        let task = tasks.create_task(Some(&title), &[], true).unwrap();
        let workspace = format!("/work/bench-{agent}");
        let change = WorkspaceChange {
            workspace: Some(workspace.clone()),
            branch: Some("b".into()),
        };
        tasks.record_workspace(&task.id, change).unwrap();
        workspaces.push(workspace);
    }
    let options = HookOptions {
        bus: None,
        workspace_changed: None,
        session_seen: None,
        signal_seen: None,
        usage_seen: None,
        home: dir.join("home"),
    };
    let service = Arc::new(HookService::new(tasks, Arc::new(NoStages), options));
    let start = Arc::new(Barrier::new(agents));
    let senders: Vec<_> = workspaces
        .into_iter()
        .map(|cwd| {
            let (service, start) = (Arc::clone(&service), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                (0..HOOKS_PER_AGENT)
                    .map(|step| {
                        let raw = agent_hook(step, &cwd);
                        let sent = Instant::now();
                        service.handle("claude-code", &raw, None, Some("1")).unwrap();
                        sent.elapsed()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    let mut times: Vec<f64> = senders
        .into_iter()
        .flat_map(|sender| sender.join().unwrap())
        .map(fine_ms)
        .collect();
    std::fs::remove_dir_all(&dir).ok();
    times.sort_by(f64::total_cmp);
    let at = |share: f64| times[((times.len() - 1) as f64 * share).round() as usize];
    (at(0.5), at(0.95))
}

/// A turn of a Claude Code agent: prompt, tool start, tool end, turn end.
fn agent_hook(step: usize, cwd: &str) -> Value {
    let name = ["UserPromptSubmit", "PreToolUse", "PostToolUse", "Stop"][step % 4];
    let mut raw = json!({ "hook_event_name": name, "cwd": cwd });
    if name.ends_with("ToolUse") {
        raw["tool_name"] = json!("Edit");
        raw["tool_input"] = json!({ "file_path": format!("{cwd}/src/file_{step}.rs") });
    }
    raw
}

/// In ms with µs steps: hooks and small loads take well under a ms.
fn fine_ms(duration: Duration) -> f64 {
    (duration.as_secs_f64() * 1_000_000.0).round() / 1000.0
}

fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("natamux-bench-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&path).ok();
    std::fs::create_dir_all(&path).unwrap();
    path
}
