//! `stage-run.ts`: what the parts of the stage runner share: its options, its state, and the
//! helpers they all use. The parts are `impl StageRun` blocks in the other files of this folder.
//!
//! The runner works on its own threads: every start, check and wait runs in a background thread,
//! as Node runs them in promises. The state is one lock, held only for lookups, never while the
//! task store, a terminal, a process or the bus is called.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, Weak};

use natamux_core::stages::terminal_lane::{PinnedTerminals, pinned_terminals_of};
use natamux_git::gh::{RunGh, run_gh};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::position::step_to_run;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::Task;
use natamux_stages::checks::{RunCommand, run_shell_command};
use natamux_stages::ci::{CiCheck, NoCi};
use serde_json::{Map, Value, json};

use crate::agents::runner::AgentRunner;
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::{Clock, Jobs, system_clock};
use crate::stages::errors::{StageResult, action};
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;
use crate::tasks::store::Applied;
use crate::terminals::open_shells::OpenShells;
use crate::terminals::service::TerminalService;

/// The folder of a repository, for git and gh.
pub type RepoPath = Arc<dyn Fn(Option<&str>) -> Result<String, TaskError> + Send + Sync>;
/// The settings of a repository (`.natamux.json`), read again at each call.
pub type SettingsOf = Arc<dyn Fn(Option<&str>) -> ProjectSettings + Send + Sync>;

pub struct StageRunOptions {
    pub tasks: TaskService,
    pub terminals: TerminalService,
    pub agents: Arc<AgentRunner>,
    pub bus: Arc<EventBus>,
    pub data_dir: PathBuf,
    pub repo_path: RepoPath,
    pub settings_of: SettingsOf,
    pub gh: RunGh,
    pub run_command: RunCommand,
    /// Pushes the work of a task to its CI branch and waits for CI. Tests give a fake.
    pub ci: Arc<dyn CiCheck>,
    pub now: Clock,
    /// The platform of the terminals' shells.
    pub windows: bool,
    /// The home folder, where agents keep the user's skills.
    pub home: PathBuf,
    /// The shells that were open, to bring back after a restart.
    pub open_shells: Option<Arc<OpenShells>>,
    pub jobs: Arc<Jobs>,
}

impl StageRunOptions {
    /// The options with Node's defaults for the rest.
    pub fn new(
        tasks: TaskService,
        terminals: TerminalService,
        agents: Arc<AgentRunner>,
        bus: Arc<EventBus>,
        data_dir: PathBuf,
        repo_path: RepoPath,
        settings_of: SettingsOf,
    ) -> StageRunOptions {
        let home = std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map_or_else(PathBuf::new, PathBuf::from);
        StageRunOptions {
            jobs: Arc::new(Jobs::new(Some(Arc::clone(&bus)), system_clock())),
            tasks,
            terminals,
            agents,
            bus,
            data_dir,
            repo_path,
            settings_of,
            gh: Arc::new(run_gh),
            run_command: Arc::new(run_shell_command),
            ci: Arc::new(NoCi),
            now: system_clock(),
            windows: cfg!(windows),
            home,
            open_shells: None,
        }
    }
}

/// An agent this daemon started in a terminal: which one, and when.
#[derive(Clone, Debug)]
pub struct Launched {
    pub agent: String,
    pub at: i64,
}

/// Work that runs in the background; others can wait for its end.
#[derive(Default)]
pub struct Work {
    done: Mutex<bool>,
    ended: Condvar,
}

impl Work {
    pub fn finish(&self) {
        *self
            .done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.ended.notify_all();
    }

    pub fn wait(&self) {
        let mut done = self
            .done
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while !*done {
            done = self
                .ended
                .wait(done)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }
}

/// The state of the runner, by "<task>" or "<task>/<terminal or stage>".
#[derive(Default)]
pub struct State {
    /// Terminals whose agent reported a stage done, with that stage: the turn end that follows
    /// the report is not a "stopped early".
    pub reported: HashMap<String, String>,
    /// Tasks whose agent stage waits for a workspace.
    pub waiting_for_workspace: HashSet<String>,
    /// Tasks whose agent stage waits for the tasks it depends on to be done.
    pub waiting_for_tasks: HashSet<String>,
    pub launched: HashMap<String, Launched>,
    /// The end of the output of those terminals, on Windows.
    pub output_tails: HashMap<String, String>,
    /// The agent a person picked, while the stage it starts is starting.
    pub chosen_agents: HashMap<String, String>,
    /// The terminal where each task runs its stage command.
    pub command_terminals: HashMap<String, String>,
    /// The step each task leaves by the send back it is sending now. The event does not say it.
    pub leaving: HashMap<String, Step>,
    /// The stages each task is starting now, so an action can answer with the task after them.
    pub start_work: HashMap<String, Arc<Work>>,
    /// The CI checks that run now, by task and stage.
    pub ci_rounds: HashSet<String>,
    /// The local checks that run now, by task, stage and round.
    pub checking: HashSet<String>,
    /// The stages that start now, by task and stage.
    pub starting: HashSet<String>,
    /// Finished tasks whose CI branch was dealt with, so later updates do not again.
    pub ci_cleaned: HashSet<String>,
}

#[derive(Default)]
struct Pending {
    count: Mutex<usize>,
    idle: Condvar,
}

/// Counts down when the background work ends, also by a panic.
struct PendingGuard(Arc<Pending>);

impl Drop for PendingGuard {
    fn drop(&mut self) {
        let mut count = self
            .0
            .count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *count = count.saturating_sub(1);
        self.0.idle.notify_all();
    }
}

pub struct StageRun {
    pub options: StageRunOptions,
    state: Mutex<State>,
    pending: Arc<Pending>,
    pub(crate) subscription: Mutex<Option<u64>>,
    this: Weak<StageRun>,
}

pub fn key(task_id: &str, part: &str) -> String {
    format!("{task_id}/{part}")
}

/// An event as a client or the runner sends it, before the task service logs it.
#[derive(Clone, Debug, PartialEq)]
pub struct EventInput {
    pub source: String,
    pub kind: String,
    pub payload: Map<String, Value>,
}

impl EventInput {
    pub fn new(source: &str, kind: &str, payload: Value) -> EventInput {
        EventInput {
            source: source.into(),
            kind: kind.into(),
            payload: match payload {
                Value::Object(map) => map,
                _ => Map::new(),
            },
        }
    }

    pub fn to_value(&self) -> Value {
        json!({ "source": self.source, "type": self.kind, "payload": self.payload })
    }
}

impl StageRun {
    pub fn new(options: StageRunOptions) -> Arc<StageRun> {
        Arc::new_cyclic(|this| StageRun {
            options,
            state: Mutex::new(State::default()),
            pending: Arc::default(),
            subscription: Mutex::new(None),
            this: this.clone(),
        })
    }

    pub fn state(&self) -> MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn weak(&self) -> Weak<StageRun> {
        self.this.clone()
    }

    /// Runs `work` on a thread of its own; `settle` waits for it.
    pub fn background(&self, work: impl FnOnce(Arc<StageRun>) + Send + 'static) {
        let Some(run) = self.this.upgrade() else {
            return;
        };
        *self
            .pending
            .count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) += 1;
        let guard = PendingGuard(Arc::clone(&self.pending));
        std::thread::spawn(move || {
            let _guard = guard;
            work(run);
        });
    }

    /// `settle` with a deadline; false when the work was still running after `limit`.
    pub fn settle_for(&self, limit: std::time::Duration) -> bool {
        let count = self
            .pending
            .count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (count, _) = self
            .pending
            .idle
            .wait_timeout_while(count, limit, |count| *count > 0)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *count == 0
    }

    /// Waits for the work the runner started in the background. For tests and shutdown.
    pub fn settle(&self) {
        let mut count = self
            .pending
            .count
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *count > 0 {
            count = self
                .pending
                .idle
                .wait(count)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    pub fn now(&self) -> i64 {
        (self.options.now)()
    }

    pub fn data_dir(&self) -> &Path {
        &self.options.data_dir
    }

    pub fn task(&self, task_id: &str) -> StageResult<Task> {
        Ok(self.options.tasks.get_task(task_id)?)
    }

    pub fn all_tasks(&self) -> Vec<Task> {
        self.options.tasks.list_tasks(None).unwrap_or_default()
    }

    pub fn steps(&self, task: &Task) -> StageResult<Vec<Step>> {
        Ok(self.options.tasks.steps_of(task)?)
    }

    pub fn events(&self, task_id: &str) -> Vec<StoredEvent> {
        self.options.tasks.list_events(task_id).unwrap_or_default()
    }

    /// Lookups use the pinned terminals, not the settings, so a settings change never moves an
    /// agent that runs.
    pub fn pinned(&self, task_id: &str) -> PinnedTerminals {
        let events = self.events(task_id);
        pinned_terminals_of(events.iter().map(|stored| (stored.accepted, &stored.event)))
    }

    pub fn settings(&self, task: &Task) -> ProjectSettings {
        (self.options.settings_of)(task.repo.as_deref())
    }

    pub fn repo_path(&self, task: &Task) -> PathBuf {
        (self.options.repo_path)(task.repo.as_deref())
            .map_or_else(|_| PathBuf::new(), PathBuf::from)
    }

    /// The step the task runs now. In a branch, that is the stage of the path it is at.
    pub fn step_of(&self, task: &Task) -> StageResult<Step> {
        let steps = self.steps(task)?;
        match step_to_run(&steps, &task.stage) {
            Some(step) if !step.is_branch() => Ok(step),
            _ => Err(action(format!(
                "{} is at an unknown stage \"{}\"",
                task.id, task.stage
            ))),
        }
    }

    /// Applies an event; its message has reached the runner and every other listener when this
    /// returns, as Node's synchronous bus does.
    pub fn apply(&self, task_id: &str, input: &EventInput) -> StageResult<Applied> {
        let applied = self
            .options
            .tasks
            .apply_event(task_id, Some(&input.to_value()));
        self.options.bus.deliver_all();
        Ok(applied?)
    }

    pub fn apply_system(&self, task_id: &str, kind: &str, payload: Value) -> StageResult<Applied> {
        self.apply(task_id, &EventInput::new("system", kind, payload))
    }

    /// An event that changes no state, such as fetched context.
    pub fn record(&self, task_id: &str, kind: &str, payload: Value) -> StageResult<Task> {
        let task = self
            .options
            .tasks
            .record_event(task_id, kind, payload, "system");
        self.options.bus.deliver_all();
        Ok(task?)
    }
}
