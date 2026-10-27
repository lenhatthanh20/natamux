//! The idle pause: which agents it pauses, and when.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_agents::hooks::Signal;
use natamux_daemon::agents::idle_pause::{IdlePause, IdlePauseOptions, PausableShells};
use natamux_daemon::agents::runner::AgentLaunch;
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use natamux_proto::tasks::task::Task;
use serde_json::{Value, json};

const T: &str = "T-0090";
/// One minute of the setting, short so the clock moves little.
const MINUTE: u64 = 1_000;

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-idle-{}-{count}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Open shells as "task/terminal", the ones a client shows, and the pauses asked for.
struct FakeShells {
    bus: Arc<EventBus>,
    open: Mutex<Vec<String>>,
    watched: Mutex<HashSet<String>>,
    refuse: AtomicBool,
    paused: Mutex<Vec<(String, u64)>>,
}

impl FakeShells {
    fn start(&self, terminal: &str) {
        self.open.lock().unwrap().push(format!("{T}/{terminal}"));
        self.bus
            .publish(DaemonMessage::ShellsChanged { task_id: T.into() });
    }
}

impl ShellsView for FakeShells {
    fn list(&self, task_id: &str) -> Vec<String> {
        let prefix = format!("{task_id}/");
        let open = self.open.lock().unwrap();
        open.iter()
            .filter_map(|key| key.strip_prefix(&prefix).map(String::from))
            .collect()
    }

    fn is_open(&self, task_id: &str, terminal: &str) -> bool {
        self.list(task_id).iter().any(|name| name == terminal)
    }

    fn foreground(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}

impl PausableShells for FakeShells {
    fn watched(&self, task_id: &str, terminal: &str) -> bool {
        self.watched
            .lock()
            .unwrap()
            .contains(&format!("{task_id}/{terminal}"))
    }

    fn pause_one(&self, task_id: &str, terminal: &str, minutes: u64) -> bool {
        if self.refuse.load(Ordering::SeqCst) || !self.is_open(task_id, terminal) {
            return false;
        }
        let key = format!("{task_id}/{terminal}");
        self.open.lock().unwrap().retain(|open| *open != key);
        self.paused.lock().unwrap().push((terminal.into(), minutes));
        self.bus.publish(DaemonMessage::ShellsChanged {
            task_id: task_id.into(),
        });
        true
    }
}

struct Fx {
    scheduler: Arc<Scheduler>,
    shells: Arc<FakeShells>,
    open: Arc<OpenShells>,
    idle: Arc<IdlePause>,
    task: Arc<Mutex<Option<Task>>>,
    minutes: Arc<Mutex<Option<u64>>>,
    runs: Arc<AtomicBool>,
    logged: Arc<Mutex<Vec<Value>>>,
    _dir: TempDir,
}

/// Copilot runs in shell 6 of the task on session c-1, and Claude Code in shell 1 on a-1. The
/// project pauses after 30 minutes.
fn fx() -> Fx {
    let dir = TempDir::new();
    let scheduler = Scheduler::manual();
    let bus = EventBus::new();
    let shells = Arc::new(FakeShells {
        bus: Arc::clone(&bus),
        open: Mutex::default(),
        watched: Mutex::default(),
        refuse: AtomicBool::new(false),
        paused: Mutex::default(),
    });
    let open = OpenShells::new(OpenShellsOptions {
        data_dir: dir.0.clone(),
        terminals: Arc::clone(&shells) as Arc<dyn ShellsView>,
        bus,
        scheduler: Arc::clone(&scheduler),
        is_ended: Arc::new(|_| false),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    open.start();
    let task = Arc::new(Mutex::new(Some(Task {
        id: T.into(),
        ..Task::default()
    })));
    let minutes = Arc::new(Mutex::new(Some(30)));
    let runs = Arc::new(AtomicBool::new(true));
    let logged: Arc<Mutex<Vec<Value>>> = Arc::default();
    let (task_of, minutes_of, agent_runs, sink) = (
        Arc::clone(&task),
        Arc::clone(&minutes),
        Arc::clone(&runs),
        Arc::clone(&logged),
    );
    let idle = IdlePause::new(IdlePauseOptions {
        scheduler: Arc::clone(&scheduler),
        minute_ms: MINUTE,
        shells: Arc::clone(&shells) as Arc<dyn PausableShells>,
        open_shells: Arc::clone(&open),
        task_of: Arc::new(move |id: &str| {
            task_of.lock().unwrap().clone().filter(|task| task.id == id)
        }),
        minutes_of: Arc::new(move |_: &Task| *minutes_of.lock().unwrap()),
        agent_runs: Arc::new(move |_: &str, _: &str| agent_runs.load(Ordering::SeqCst)),
        on_paused: Arc::new(move |_: &str, _: &str, payload: Value| {
            sink.lock().unwrap().push(payload);
        }),
    });
    idle.start();
    for (terminal, agent, session) in [("1", "claude-code", "a-1"), ("6", "copilot", "c-1")] {
        shells.start(terminal);
        open.note_session(T, terminal, agent, session);
    }
    Fx {
        scheduler,
        shells,
        open,
        idle,
        task,
        minutes,
        runs,
        logged,
        _dir: dir,
    }
}

impl Fx {
    fn minutes_pass(&self, minutes: u64) {
        self.scheduler.advance(minutes * MINUTE);
    }

    fn signal(&self, terminal: &str, signal: Signal) {
        self.idle.note_signal(T, terminal, signal);
    }

    fn pauses(&self) -> Vec<(String, u64)> {
        self.shells.paused.lock().unwrap().clone()
    }
}

#[test]
fn pauses_an_agent_whose_turn_ended_30_minutes_ago_and_that_no_one_shows() {
    let f = fx();
    f.signal("6", Signal::TurnEnded);
    f.signal("1", Signal::Activity);

    f.minutes_pass(29);
    assert!(f.pauses().is_empty());

    f.minutes_pass(1);
    assert_eq!(f.pauses(), [("6".to_string(), 30)]);
    assert_eq!(
        *f.logged.lock().unwrap(),
        [json!({ "terminal": "6", "agent": "copilot", "session": "c-1", "idleMinutes": 30 })]
    );
    assert!(f.open.is_paused(T, "6"));
    assert_eq!(f.open.waiting(T), ["6"]);
    // Other shells run on, and a pause comes once.
    assert!(f.shells.is_open(T, "1"));
    f.minutes_pass(60);
    assert_eq!(f.pauses().len(), 1);
}

#[test]
fn waits_the_minutes_the_project_sets_and_never_pauses_when_it_is_off() {
    let f = fx();
    *f.minutes.lock().unwrap() = Some(5);
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(5);
    assert_eq!(f.pauses(), [("6".to_string(), 5)]);

    let f = fx();
    *f.minutes.lock().unwrap() = None;
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(120);
    assert!(f.pauses().is_empty());
}

#[test]
fn does_not_pause_an_agent_whose_last_signal_is_not_a_turn_end() {
    for signal in [
        Signal::Prompt,
        Signal::Activity,
        Signal::Asking,
        Signal::TurnPaused,
        Signal::Failed,
    ] {
        let f = fx();
        f.signal("6", signal);
        f.minutes_pass(60);
        assert!(f.pauses().is_empty(), "{signal:?}");
    }
}

#[test]
fn counts_the_idle_time_from_the_last_turn_end() {
    let f = fx();
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(20);
    f.signal("6", Signal::Prompt);
    f.minutes_pass(20);
    f.signal("6", Signal::TurnEnded);

    f.minutes_pass(29);
    assert!(f.pauses().is_empty());
    f.minutes_pass(1);
    assert_eq!(f.pauses().len(), 1);
}

#[test]
fn does_not_pause_while_the_task_waits_for_background_work() {
    let f = fx();
    if let Some(task) = f.task.lock().unwrap().as_mut() {
        task.background_since = Some("2026-10-27T09:00:00.000Z".into());
    }
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());
}

#[test]
fn waits_while_a_client_shows_the_terminal_and_pauses_once_none_does() {
    let f = fx();
    f.shells.watched.lock().unwrap().insert(format!("{T}/6"));
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());

    f.shells.watched.lock().unwrap().clear();
    f.minutes_pass(1);
    assert_eq!(f.pauses().len(), 1);
}

#[test]
fn does_not_pause_a_shell_with_no_running_agent_or_no_known_session() {
    let f = fx();
    f.runs.store(false, Ordering::SeqCst);
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());

    let f = fx();
    f.shells.start("7");
    f.open.note_launch(T, "7", "cursor", None);
    f.signal("7", Signal::TurnEnded);
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());
}

#[test]
fn does_not_pause_an_agent_of_a_task_that_is_done_cancelled_or_gone() {
    let f = fx();
    *f.task.lock().unwrap() = None;
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());
}

#[test]
fn lets_the_shell_run_on_when_it_cannot_be_closed() {
    let f = fx();
    f.shells.refuse.store(true, Ordering::SeqCst);
    f.signal("6", Signal::TurnEnded);
    f.minutes_pass(30);

    assert!(f.logged.lock().unwrap().is_empty());
    assert!(!f.open.is_paused(T, "6"));
    assert!(f.open.waiting(T).is_empty());
}

fn launch(terminal: &str, restored: bool) -> AgentLaunch {
    AgentLaunch {
        task_id: T.into(),
        terminal: terminal.into(),
        agent: "copilot".into(),
        program: "copilot".into(),
        session: Some("c-1".into()),
        restored,
    }
}

#[test]
fn counts_a_restored_launch_as_a_turn_end_and_a_launch_with_a_prompt_not() {
    let f = fx();
    f.idle.note_launch(&launch("6", true));
    f.minutes_pass(30);
    assert_eq!(f.pauses(), [("6".to_string(), 30)]);

    let f = fx();
    f.idle.note_launch(&launch("6", false));
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());
}

#[test]
fn pauses_nothing_once_stopped() {
    let f = fx();
    f.signal("6", Signal::TurnEnded);
    f.idle.stop();
    f.minutes_pass(60);
    assert!(f.pauses().is_empty());
}
