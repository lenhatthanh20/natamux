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

/// The daemon's RSS in MiB once 30 shells each printed 20 MB, from 30 threads at once, how much
/// it grew over the RSS before the shells, and that growth for one shell.
pub fn busy_shells() -> (f64, f64, f64) {
    let before = rss_kib().unwrap_or(0);
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
    let mib = |kib: u64| (kib as f64 / 1024.0 * 10.0).round() / 10.0;
    let grown = rss.saturating_sub(before);
    (mib(rss), mib(grown), mib(grown / shells as u64))
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

pub fn bench_task(number: usize) -> Task {
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

/// What a hook bench sends its hooks to.
pub struct HookBench {
    pub service: Arc<HookService>,
    /// The folder of each agent's task.
    pub workspaces: Vec<String>,
    /// The home folder when hooks name a session and its usage file there, as Claude Code's do.
    pub sessions: Option<PathBuf>,
}

/// One agent sends 50 hooks a second, far more than a real agent: start delay, pause.
pub const PACE: (Duration, Duration) = (Duration::from_micros(700), Duration::from_millis(20));

/// A hook service over `agents` tasks, each with its own workspace, with no stages and nothing
/// told of a hook; the folder of the bench.
fn hook_setup(agents: usize) -> (Arc<HookBench>, PathBuf) {
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
    let bench = HookBench {
        service,
        workspaces,
        sessions: None,
    };
    (Arc::new(bench), dir)
}

/// The hooks of agent `agent`: `HOOKS_PER_AGENT` hooks of its folder, `pause` between two.
fn send_hooks(bench: &HookBench, agent: usize, pause: Option<Duration>) -> Vec<Duration> {
    let cwd = &bench.workspaces[agent];
    (0..HOOKS_PER_AGENT)
        .map(|step| {
            let mut raw = agent_hook(step, cwd);
            if let Some(home) = &bench.sessions {
                raw["session_id"] = json!(format!("bench-session-{agent}"));
                let file = home.join(format!(".claude/projects/bench/{agent}.jsonl"));
                raw["transcript_path"] = json!(file);
            }
            let sent = Instant::now();
            bench
                .service
                .handle("claude-code", &raw, None, Some("1"))
                .unwrap();
            let took = sent.elapsed();
            if let Some(pause) = pause {
                std::thread::sleep(pause);
            }
            took
        })
        .collect()
}

/// The hook times in ms. `max` and `over_50` show a rare stall that p95 hides.
#[derive(Clone, Copy)]
pub struct HookTimes {
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
    pub over_50: usize,
}

fn hook_times(times: Vec<Duration>) -> HookTimes {
    let mut times: Vec<f64> = times.into_iter().map(fine_ms).collect();
    times.sort_by(f64::total_cmp);
    let at = |share: f64| times[((times.len() - 1) as f64 * share).round() as usize];
    HookTimes {
        p50: at(0.5),
        p95: at(0.95),
        max: at(1.0),
        over_50: times.iter().filter(|time| **time > 50.0).count(),
    }
}

/// One thread per agent. `pace`: the start of agent `i` is delayed by `i * stagger` and each
/// agent waits `pause` after each hook; None: all start at once and send back to back.
pub fn times_from_threads(bench: &Arc<HookBench>, pace: Option<(Duration, Duration)>) -> HookTimes {
    let agents = bench.workspaces.len();
    let start = Arc::new(Barrier::new(agents));
    let senders: Vec<_> = (0..agents)
        .map(|agent| {
            let (bench, start) = (Arc::clone(bench), Arc::clone(&start));
            std::thread::spawn(move || {
                start.wait();
                if let Some((stagger, _)) = pace {
                    std::thread::sleep(stagger * agent as u32);
                }
                send_hooks(&bench, agent, pace.map(|(_, pause)| pause))
            })
        })
        .collect();
    let times = senders
        .into_iter()
        .flat_map(|sender| sender.join().unwrap())
        .collect();
    hook_times(times)
}

/// The hooks from one thread, one agent after the other: the work of a hook, no queue.
pub fn times_single(bench: &HookBench) -> HookTimes {
    let agents = bench.workspaces.len();
    let times = (0..agents)
        .flat_map(|agent| send_hooks(bench, agent, None))
        .collect();
    hook_times(times)
}

/// `run` on a new bench of `agents` agents with nothing wired, then its folder goes.
fn bare(agents: usize, run: impl FnOnce(&Arc<HookBench>) -> HookTimes) -> HookTimes {
    let (bench, dir) = hook_setup(agents);
    let times = run(&bench);
    std::fs::remove_dir_all(&dir).ok();
    times
}

/// `HookService::handle` while `agents` agents each send hooks from their own thread, all at
/// once: p50 and p95 in ms. The task is found by the hook's working directory. The time of a hook
/// here holds the wait for a core and for the store lock: 30 threads on 4 cores, no pause.
pub fn hooks_at_once(agents: usize) -> HookTimes {
    bare(agents, |bench| times_from_threads(bench, None))
}

/// The same hooks from one thread, one agent after the other: the work of a hook, no queue.
pub fn hooks_single(agents: usize) -> HookTimes {
    bare(agents, |bench| times_single(bench))
}

/// As `hooks_at_once`, but paced as `PACE`: what an agent waits.
pub fn hooks_paced(agents: usize) -> HookTimes {
    bare(agents, |bench| times_from_threads(bench, Some(PACE)))
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

pub fn temp_dir(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!("natamux-bench-{}-{name}", std::process::id()));
    std::fs::remove_dir_all(&path).ok();
    std::fs::create_dir_all(&path).unwrap();
    path
}
