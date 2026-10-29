//! `workspace-service.test.ts`, ported: one file per `describe`. The fixture is a real git
//! repository, the real task service and project list, and a workspace service whose gh, agent run,
//! log, `close_terminals` and `on_created` are stubs that record what they were given.
//!
//! Node's fake timers are the manual `Scheduler`; its injected `now` is a clock a test moves by
//! hand. Node's `vi.spyOn(workspaces, 'touch')` cannot be ported: a test checks the real effect (a
//! `workspace.dirty` message after the scheduler advances) and says so. Node's promises that run
//! side by side are threads; a test that needs a change to be running holds it with `Gate`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod base_branch;
mod changes;
mod create;
mod diff_stats;
mod draft;
mod graph;
mod jobs;
mod on_created;
mod publish;
mod remove;
mod remove_refusals;
mod remove_stage_files;
mod remove_trust;
mod rollback;
mod support;
mod touch;

use std::fs;
use std::ops::Deref;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex, RwLock, RwLockReadGuard, RwLockWriteGuard};
use std::time::Duration;

use natamux_daemon::projects::repos::Repos;
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::jobs::{Clock, Jobs};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::workspaces::errors::WorkspaceError;
use natamux_daemon::workspaces::service::{WorkspaceOptions, WorkspaceService};
use natamux_git::test_support::{TempDir, test_repo};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::workspaces::workspace::DiffStat;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

pub use natamux_git::test_support::{bare_origin, run_git};

pub use support::*;

pub const PR_URL: &str = "https://github.com/owner/repo/pull/7";
/// The branch of the first task, titled "Fix the bug".
pub const BRANCH: &str = "natamux/t-0001-fix-the-bug";

pub type Messages = Arc<Mutex<Vec<DaemonMessage>>>;

// Tests that count git processes (`spawn_count` is for the whole binary) take the write side, so no
// other test runs git at that time. Every other test takes the read side.
static GIT_LOCK: RwLock<()> = RwLock::new(());

// The guards are held, never read.
#[allow(dead_code)]
enum Guard {
    Shared(RwLockReadGuard<'static, ()>),
    Exclusive(RwLockWriteGuard<'static, ()>),
}

/// Everything of a test except its default workspace service.
/// The terminals closed before a removal, with what was asked of each.
pub type Closed = Arc<Mutex<Vec<(String, Option<String>)>>>;

pub struct Base {
    /// The folder that holds the repository and anything else a test needs next to it.
    pub root: TempDir,
    pub repo: PathBuf,
    pub data: PathBuf,
    pub worktrees: PathBuf,
    pub tasks: TaskService,
    /// What the bus of the task service gave (Node's `messages`).
    pub task_messages: Messages,
    pub repos: Arc<Repos>,
    /// The bus of the workspace service, and what it gave.
    pub bus: Arc<EventBus>,
    pub sent: Messages,
    pub jobs: Arc<Jobs>,
    pub scheduler: Arc<Scheduler>,
    /// The clock of the git reads that are kept for a moment, in ms.
    pub clock: Arc<AtomicI64>,
    pub log: Arc<Mutex<Vec<String>>>,
    /// Each `close_terminals` call: the task, and its workspace at that time.
    pub closed: Closed,
    /// The arguments of each gh call.
    pub gh_calls: Arc<Mutex<Vec<Vec<String>>>>,
}

pub struct Fx {
    pub ws: Arc<WorkspaceService>,
    base: Base,
    _guard: Guard,
}

impl Deref for Fx {
    type Target = Base;

    fn deref(&self) -> &Base {
        &self.base
    }
}

impl Default for Fx {
    fn default() -> Fx {
        Fx::new()
    }
}

impl Fx {
    /// A task with its workspace, made by the default service.
    pub fn with_workspace(&self, title: &str) -> Task {
        self.made_by(&self.ws, title)
    }

    pub fn new() -> Fx {
        let guard = Guard::Shared(GIT_LOCK.read().unwrap_or_else(|e| e.into_inner()));
        Fx::with_guard(guard)
    }

    /// For a test that counts git processes.
    pub fn exclusive() -> Fx {
        let guard = Guard::Exclusive(GIT_LOCK.write().unwrap_or_else(|e| e.into_inner()));
        Fx::with_guard(guard)
    }

    fn with_guard(guard: Guard) -> Fx {
        let (root, repo) = test_repo();
        let data = root.path().join("data");
        fs::create_dir(&data).unwrap();
        let worktrees = root.path().join("worktrees");
        fs::write(
            data.join("repos.json"),
            json!([{ "id": "default", "name": "default", "path": repo }]).to_string(),
        )
        .unwrap();
        let clock = Arc::new(AtomicI64::new(0));
        let now = clock_of(&clock);
        let task_bus = EventBus::new();
        let task_messages: Messages = Arc::default();
        record(&task_bus, &task_messages);
        let tasks = TaskService::new(TaskServiceOptions {
            data_dir: data.clone(),
            index: TaskIndex::open(Path::new(":memory:")).unwrap(),
            bus: task_bus,
            now: None,
            settings_of: None,
            clock: None,
        });
        let bus = EventBus::new();
        let sent: Messages = Arc::default();
        record(&bus, &sent);
        let base = Base {
            repos: Repos::new(&data, Arc::clone(&now)),
            jobs: Arc::new(Jobs::new(None, Arc::clone(&now))),
            scheduler: Scheduler::manual(),
            log: Arc::default(),
            closed: Arc::default(),
            gh_calls: Arc::default(),
            root,
            repo,
            data,
            worktrees,
            tasks,
            task_messages,
            bus,
            sent,
            clock,
        };
        let ws = WorkspaceService::new(base.options());
        Fx {
            ws,
            base,
            _guard: guard,
        }
    }
}

pub fn clock_of(time: &Arc<AtomicI64>) -> Clock {
    let read = Arc::clone(time);
    Arc::new(move || read.load(Ordering::SeqCst))
}

/// Keeps what the bus gives.
pub fn record(bus: &EventBus, into: &Messages) {
    let sink = Arc::clone(into);
    bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
}

impl Base {
    /// The options with every callback recording, and a gh that answers with `PR_URL`.
    pub fn options(&self) -> WorkspaceOptions {
        let mut options = WorkspaceOptions::new(
            self.data.clone(),
            self.worktrees.clone(),
            self.tasks.clone(),
            Arc::clone(&self.repos),
            Arc::clone(&self.bus),
            Arc::clone(&self.jobs),
            Arc::clone(&self.scheduler),
        );
        options.now = clock_of(&self.clock);
        let (tasks, closed) = (self.tasks.clone(), Arc::clone(&self.closed));
        options.close_terminals = Arc::new(move |task_id| {
            let workspace = tasks.get_task(task_id).unwrap().workspace;
            closed
                .lock()
                .unwrap()
                .push((task_id.to_string(), workspace));
        });
        let calls = Arc::clone(&self.gh_calls);
        options.run_gh = Arc::new(move |_cwd: &Path, args: &[String]| {
            calls.lock().unwrap().push(args.to_vec());
            Ok(format!("{PR_URL}\n"))
        });
        let log = Arc::clone(&self.log);
        options.log = Arc::new(move |message| log.lock().unwrap().push(message.to_string()));
        options
    }

    /// Another service on the same repository, with `change` applied to the options.
    pub fn build(&self, change: impl FnOnce(&mut WorkspaceOptions)) -> Arc<WorkspaceService> {
        let mut options = self.options();
        change(&mut options);
        WorkspaceService::new(options)
    }

    pub fn task(&self, title: &str) -> Task {
        self.task_with(json!({ "title": title }))
    }

    pub fn task_with(&self, input: Value) -> Task {
        self.tasks.create_task(Some(&input), &[], true).unwrap()
    }

    /// A task with its workspace, made by `service`.
    pub fn made_by(&self, service: &WorkspaceService, title: &str) -> Task {
        service.create(&self.task(title).id, None).unwrap()
    }

    pub fn event(&self, task_id: &str, source: &str, kind: &str) {
        let input = json!({ "source": source, "type": kind });
        self.tasks.apply_event(task_id, Some(&input)).unwrap();
    }

    /// The agent of the task runs (state `running`).
    pub fn start_agent(&self, task_id: &str) {
        self.event(task_id, "human", "task.readied");
        self.event(task_id, "agent", "agent.started");
    }

    pub fn override_state(&self, task_id: &str, to: &str) {
        let input =
            json!({ "source": "human", "type": "state.overridden", "payload": { "to": to } });
        self.tasks.apply_event(task_id, Some(&input)).unwrap();
    }

    pub fn events(&self, task_id: &str) -> Vec<StoredEvent> {
        self.tasks.list_events(task_id).unwrap()
    }

    pub fn event_kinds(&self, task_id: &str) -> Vec<String> {
        self.events(task_id)
            .into_iter()
            .map(|stored| stored.event.kind)
            .collect()
    }

    pub fn state_of(&self, task_id: &str) -> TaskState {
        self.tasks.get_task(task_id).unwrap().state
    }

    pub fn workspace_of(&self, task_id: &str) -> Option<String> {
        self.tasks.get_task(task_id).unwrap().workspace
    }

    pub fn branch_exists(&self, branch: &str) -> bool {
        !run_git(&self.repo, &["branch", "--list", branch])
            .trim()
            .is_empty()
    }

    pub fn move_clock(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    /// How many `workspace.dirty` messages the workspace bus gave for the task.
    pub fn dirty_count(&self, task_id: &str) -> usize {
        self.sent
            .lock()
            .unwrap()
            .iter()
            .filter(|m| matches!(m, DaemonMessage::WorkspaceDirty { task_id: id } if id == task_id))
            .count()
    }

    /// The tasks whose shells `close_terminals` was asked to end, in order.
    pub fn closed_ids(&self) -> Vec<String> {
        let closed = self.closed.lock().unwrap();
        closed.iter().map(|(id, _)| id.clone()).collect()
    }

    pub fn log_lines(&self) -> Vec<String> {
        self.log.lock().unwrap().clone()
    }

    pub fn gh_args(&self) -> Vec<Vec<String>> {
        self.gh_calls.lock().unwrap().clone()
    }

    /// Runs the timers of `workspace.dirty` that are due, so a test sees what a `touch` made.
    pub fn flush_dirty(&self) {
        self.scheduler
            .advance(natamux_daemon::workspaces::dirty::DIRTY_DEBOUNCE_MS);
    }
}
