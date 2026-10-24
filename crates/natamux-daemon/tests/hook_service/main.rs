//! `agent-hook-service.test.ts`, ported: one file per `describe` group and the fixture of its
//! `beforeEach`. Node's callbacks that tests spied on (`workspaceChanged`, `sessionSeen`,
//! `usageSeen`) are closures that record what they got. Part 5 gives `workspaceChanged` its real
//! effect (`workspaces.touch`); here the service only calls it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// `Fx::new` is `beforeEach`.
#![allow(clippy::new_without_default)]

mod adapter_list;
mod background_work;
mod basics;
mod failed_group;
mod pushed;
mod sign_of_life;
mod workspace_removed;

use std::path::PathBuf;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_daemon::agents::hook_service::{
    HookOptions, HookResult, HookService, Lane, NoStages, StageLookup,
};
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::jobs::Clock;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::tasks::store::SettingsOf;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

/// A fresh folder for one test, removed when the test ends.
pub struct TempDir(pub PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-hooks-{}-{count}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// 2026-10-08T09:00:00Z
pub fn base_millis() -> i64 {
    natamux_core::js::parse_iso_millis("2026-10-08T09:00:00.000Z").unwrap()
}

/// A clock a test moves by hand.
pub fn manual_clock() -> (Arc<AtomicI64>, Clock) {
    let time = Arc::new(AtomicI64::new(base_millis()));
    let read = Arc::clone(&time);
    (time, Arc::new(move || read.load(Ordering::SeqCst)))
}

pub struct Fx {
    pub dir: TempDir,
    pub bus: Arc<EventBus>,
    /// Every message on the bus, in order.
    pub sent: Arc<Mutex<Vec<DaemonMessage>>>,
    pub tasks: TaskService,
}

impl Fx {
    /// `beforeEach`: a task service on the legacy pipeline and the system clock.
    pub fn new() -> Fx {
        Fx::build(None, None)
    }

    pub fn build(settings: Option<SettingsOf>, now: Option<Clock>) -> Fx {
        let dir = TempDir::new();
        let index = TaskIndex::open(&dir.0.join("index.sqlite")).unwrap();
        let bus = EventBus::new();
        let sent: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
        let sink = Arc::clone(&sent);
        bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
        let tasks = TaskService::new(TaskServiceOptions {
            data_dir: dir.0.clone(),
            index,
            bus: Arc::clone(&bus),
            now,
            settings_of: settings,
            clock: None,
        });
        Fx {
            dir,
            bus,
            sent,
            tasks,
        }
    }

    /// `new AgentHookService(tasks)`: no stages, no bus, no callbacks.
    pub fn service(&self) -> HookService {
        self.service_with(Arc::new(NoStages), self.options(false))
    }

    pub fn service_with(&self, stages: Arc<dyn StageLookup>, options: HookOptions) -> HookService {
        HookService::new(self.tasks.clone(), stages, options)
    }

    pub fn options(&self, bus: bool) -> HookOptions {
        HookOptions {
            bus: bus.then(|| Arc::clone(&self.bus)),
            workspace_changed: None,
            session_seen: None,
            usage_seen: None,
            home: self.dir.0.join("home"),
        }
    }

    pub fn make(&self, title: &str) -> Task {
        self.tasks
            .create_task(Some(&json!({ "title": title })), &[], true)
            .unwrap()
    }

    /// `tasks.recordWorkspace(id, { workspace, branch: 'b' })`.
    pub fn workspace(&self, id: &str, workspace: Option<&str>) {
        let change = WorkspaceChange {
            workspace: workspace.map(String::from),
            branch: Some("b".into()),
        };
        self.tasks.record_workspace(id, change).unwrap();
    }

    pub fn task(&self, id: &str) -> Task {
        self.tasks.get_task(id).unwrap()
    }

    pub fn state(&self, id: &str) -> TaskState {
        self.task(id).state
    }

    pub fn kinds(&self, id: &str) -> Vec<String> {
        self.tasks
            .list_events(id)
            .unwrap()
            .into_iter()
            .map(|stored| stored.event.kind)
            .collect()
    }

    pub fn event_count(&self, id: &str) -> usize {
        self.tasks.list_events(id).unwrap().len()
    }

    /// The payload of the last event of the task.
    pub fn last_payload(&self, id: &str) -> serde_json::Map<String, Value> {
        self.tasks
            .list_events(id)
            .unwrap()
            .pop()
            .unwrap()
            .event
            .payload
    }

    pub fn apply(&self, id: &str, input: Value) {
        self.tasks.apply_event(id, Some(&input)).unwrap();
    }

    pub fn messages(&self) -> Vec<DaemonMessage> {
        self.sent.lock().unwrap().clone()
    }
}

/// `{ hook_event_name: name, ...extra }`
pub fn hook(name: &str, extra: Value) -> Value {
    let mut raw = json!({ "hook_event_name": name });
    if let (Some(map), Some(more)) = (raw.as_object_mut(), extra.as_object()) {
        for (key, value) in more {
            map.insert(key.clone(), value.clone());
        }
    }
    raw
}

/// A Claude Code hook for a task, from `terminal` (None: no terminal header).
pub fn claude(
    service: &HookService,
    task: &str,
    raw: &Value,
    terminal: Option<&str>,
) -> HookResult {
    service
        .handle("claude-code", raw, Some(task), terminal)
        .unwrap()
}

/// A Claude Code hook with just a name, from no terminal.
pub fn step(service: &HookService, task: &str, name: &str) -> HookResult {
    claude(service, task, &hook(name, json!({})), None)
}

/// `{ taskId, applied: [] }` as a result.
pub fn nothing_for(task: &str) -> HookResult {
    HookResult {
        task_id: Some(task.to_string()),
        applied: Vec::new(),
    }
}

/// `{ taskId: null, applied: [] }`
pub fn no_task() -> HookResult {
    HookResult {
        task_id: None,
        applied: Vec::new(),
    }
}

/// The text of the activity line of a terminal.
pub fn line(service: &HookService, task: &str, terminal: &str) -> Option<String> {
    service
        .activity()
        .get(task)?
        .get(terminal)?
        .get("text")?
        .as_str()
        .map(String::from)
}

/// The terminals that have an activity line, in order.
pub fn line_terminals(service: &HookService, task: &str) -> Vec<String> {
    service
        .activity()
        .get(task)
        .and_then(Value::as_object)
        .map(|lines| lines.keys().cloned().collect())
        .unwrap_or_default()
}

/// The task's own stage and state: the lane of a stage runner that has no group.
pub fn own_lane(task: &Task) -> Option<Lane> {
    Some((task.stage.clone(), task.state))
}

type LaneFn = dyn Fn(&Task, Option<&str>) -> Option<Lane> + Send + Sync;
type ReportFn = dyn Fn(&str, &str) -> bool + Send + Sync;

/// A `StageLookup` made of two closures, as the tests of Node make one of two functions.
pub struct Mock {
    lane: Box<LaneFn>,
    report: Box<ReportFn>,
}

impl StageLookup for Mock {
    fn lane_for(&self, task: &Task, terminal: Option<&str>) -> Result<Option<Lane>, String> {
        Ok((self.lane)(task, terminal))
    }

    fn take_report(&self, task_id: &str, terminal: &str) -> bool {
        (self.report)(task_id, terminal)
    }
}

pub fn mock(
    lane: impl Fn(&Task, Option<&str>) -> Option<Lane> + Send + Sync + 'static,
    report: impl Fn(&str, &str) -> bool + Send + Sync + 'static,
) -> Arc<dyn StageLookup> {
    Arc::new(Mock {
        lane: Box::new(lane),
        report: Box::new(report),
    })
}
