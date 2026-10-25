//! `task-service.test.ts`, ported: one file per `describe` group. Node's spies (`vi.spyOn`) cannot
//! be ported; where a test used one, it checks the real effect instead and says so. A test that
//! shared one `TaskIndex` between services opens `<data>/index.sqlite` once per service.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
// Each group uses a part of the helpers.
#![allow(dead_code)]

mod apply_branch;
mod apply_event;
mod apply_group;
mod broken_file;
mod create_task;
mod event_log_files;
mod init;
mod init_from_index;
mod is_task_ended;
mod lookups;
mod outside_edits;
mod polled;
mod pull_request;
mod record_activity;
mod records;
mod set_workflow;
mod settings_change;
mod stages_off;
mod stop_agents;
mod subtasks;
mod tasks_in_memory;
mod update_task;

use std::fmt::Debug;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_core::js::parse_iso_millis;
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::jobs::Clock;
use natamux_daemon::tasks::errors::TaskResult;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::tasks::store::{Applied, SettingsOf};
use natamux_proto::events::task_event::{StoredEvent, TaskEvent};
use natamux_proto::stages::schema::{Stage, Step};
use natamux_proto::tasks::task::Task;
use natamux_store::task_index::TaskIndex;
use serde_json::{Map, Value, json};

/// A fresh folder for one test, removed when the test ends.
pub struct TempDir(pub PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-service-{}-{count}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        TempDir(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

pub type Messages = Arc<Mutex<Vec<DaemonMessage>>>;

/// 2026-10-08T09:00:00Z, the start of the clock of every test.
pub fn base_millis() -> i64 {
    parse_iso_millis("2026-10-08T09:00:00.000Z").unwrap()
}

/// One minute more per call, like Node's `minute++`.
pub fn minute_clock() -> Clock {
    let minute = AtomicI64::new(0);
    Arc::new(move || base_millis() + minute.fetch_add(1, Ordering::SeqCst) * 60_000)
}

/// A clock a test moves by hand.
pub fn manual_clock(start: i64) -> (Arc<AtomicI64>, Clock) {
    let time = Arc::new(AtomicI64::new(start));
    let read = Arc::clone(&time);
    (time, Arc::new(move || read.load(Ordering::SeqCst)))
}

pub struct Fixture {
    pub dir: TempDir,
    pub messages: Messages,
    /// The service of `beforeEach`: it uses the file index and the legacy pipeline.
    pub service: TaskService,
}

/// What a test may set when it builds another service.
#[derive(Default)]
pub struct Build {
    pub settings: Option<SettingsOf>,
    pub now: Option<Clock>,
    pub clock: Option<Clock>,
}

impl Default for Fixture {
    fn default() -> Fixture {
        Fixture::new()
    }
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = TempDir::new();
        let messages: Messages = Arc::default();
        let index = TaskIndex::open(&dir.0.join("index.sqlite")).unwrap();
        let service = build(&dir.0, index, &messages, Build::default());
        Fixture {
            dir,
            messages,
            service,
        }
    }

    pub fn data(&self) -> &Path {
        &self.dir.0
    }

    /// Another connection to the index the fixture's services share.
    pub fn index(&self) -> TaskIndex {
        TaskIndex::open(&self.dir.0.join("index.sqlite")).unwrap()
    }

    pub fn make(&self, settings: Option<SettingsOf>) -> TaskService {
        build(
            &self.dir.0,
            self.index(),
            &self.messages,
            Build {
                settings,
                ..Build::default()
            },
        )
    }

    /// A service on the shared index, with the minute clock unless `with` gives its own.
    pub fn with(&self, with: Build) -> TaskService {
        build(&self.dir.0, self.index(), &self.messages, with)
    }

    /// A service after the start of the daemon on the shared index. It stops watching the files at
    /// once, so a test can remove them.
    pub fn started(&self, settings: Option<SettingsOf>) -> TaskService {
        self.started_on(self.index(), settings)
    }

    pub fn started_on(&self, index: TaskIndex, settings: Option<SettingsOf>) -> TaskService {
        let service = build(
            &self.dir.0,
            index,
            &self.messages,
            Build {
                settings,
                ..Build::default()
            },
        );
        service.init().unwrap();
        service.close();
        service
    }

    pub fn memory_index(&self) -> TaskIndex {
        TaskIndex::open(Path::new(":memory:")).unwrap()
    }

    pub fn messages(&self) -> Vec<DaemonMessage> {
        self.messages.lock().unwrap().clone()
    }

    pub fn clear_messages(&self) {
        self.messages.lock().unwrap().clear();
    }
}

pub fn build(data: &Path, index: TaskIndex, messages: &Messages, with: Build) -> TaskService {
    let bus = EventBus::new();
    let seen = Arc::clone(messages);
    bus.subscribe(move |message| seen.lock().unwrap().push(message.clone()));
    TaskService::new(TaskServiceOptions {
        data_dir: data.to_path_buf(),
        index,
        bus,
        now: Some(with.now.unwrap_or_else(minute_clock)),
        settings_of: with.settings,
        clock: with.clock,
    })
}

pub fn obj(value: Value) -> Map<String, Value> {
    match value {
        Value::Object(map) => map,
        other => panic!("not an object: {other}"),
    }
}

pub fn steps(value: Value) -> Vec<Step> {
    serde_json::from_value(value).unwrap()
}

pub fn settings(stages: Vec<Step>) -> SettingsOf {
    Arc::new(move |_| stages.clone())
}

/// Stages a test changes while a service runs.
pub fn live_settings(stages: Vec<Step>) -> (Arc<Mutex<Vec<Step>>>, SettingsOf) {
    let shared = Arc::new(Mutex::new(stages));
    let read = Arc::clone(&shared);
    (shared, Arc::new(move |_| read.lock().unwrap().clone()))
}

pub fn stage_of<'a>(steps: &'a [Step], id: &str) -> &'a Stage {
    steps
        .iter()
        .flat_map(Step::stages)
        .find(|stage| stage.id == id)
        .unwrap()
}

pub fn create(service: &TaskService, title: &str) -> Task {
    service
        .create_task(Some(&json!({ "title": title })), &[], true)
        .unwrap()
}

pub fn create_with(service: &TaskService, input: Value) -> Task {
    service.create_task(Some(&input), &[], true).unwrap()
}

pub fn apply(service: &TaskService, id: &str, input: Value) -> Applied {
    service.apply_event(id, Some(&input)).unwrap()
}

/// A human event with no payload.
pub fn human(service: &TaskService, id: &str, kind: &str) -> Applied {
    apply(service, id, json!({ "source": "human", "type": kind }))
}

/// A person moves the task to `to` (`state.overridden`).
pub fn override_to(service: &TaskService, id: &str, to: &str, stage: Option<&str>) -> Applied {
    let mut payload = json!({ "to": to });
    if let Some(stage) = stage {
        payload["stage"] = json!(stage);
    }
    apply(
        service,
        id,
        json!({ "source": "human", "type": "state.overridden", "payload": payload }),
    )
}

pub fn kinds(events: &[StoredEvent]) -> Vec<String> {
    events
        .iter()
        .map(|stored| stored.event.kind.clone())
        .collect()
}

pub fn event_kinds(service: &TaskService, id: &str) -> Vec<String> {
    kinds(&service.list_events(id).unwrap())
}

pub fn last_event(service: &TaskService, id: &str) -> StoredEvent {
    service.list_events(id).unwrap().pop().unwrap()
}

pub fn last_payload(service: &TaskService, id: &str) -> Map<String, Value> {
    last_event(service, id).event.payload
}

pub fn ids(tasks: &[Task]) -> Vec<String> {
    tasks.iter().map(|task| task.id.clone()).collect()
}

pub fn titles(tasks: &[Task]) -> Vec<String> {
    tasks.iter().map(|task| task.title.clone()).collect()
}

/// The text of the error of a call that must fail.
pub fn err_text<T: Debug>(result: TaskResult<T>) -> String {
    result.unwrap_err().to_string()
}

pub fn event_pairs(events: &[StoredEvent]) -> Vec<(TaskEvent, bool)> {
    events
        .iter()
        .map(|stored| (stored.event.clone(), stored.accepted))
        .collect()
}

/// The last message the bus gave.
pub fn last_message(fx: &Fixture) -> DaemonMessage {
    fx.messages().pop().unwrap()
}

pub fn file_task(fx: &Fixture, id: &str) -> Option<Task> {
    natamux_store::task_files::read_task_file(fx.data(), id).unwrap()
}

/// What the index of the service holds.
pub fn index_tasks(service: &TaskService) -> Vec<Task> {
    service
        .with(|store| Ok(store.sync.index.list_tasks(None).unwrap()))
        .unwrap()
}
