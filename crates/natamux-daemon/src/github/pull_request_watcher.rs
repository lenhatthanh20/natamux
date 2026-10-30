//! `pull-request-watcher.ts`: asks GitHub about the pull requests of tasks, so a task shows its CI
//! and moves to done when its pull request is merged. One gh call per GitHub repository. It also
//! links a pull request that the agent or a person opened from the task branch outside Natamux,
//! with one gh call per project.
//!
//! Threads: the timer (on the `Scheduler` thread), `wake` and the bus listener only put work on a
//! channel. One worker thread takes it and runs gh. `check` runs on the caller's thread.

use std::collections::HashSet;
use std::path::Path;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, PoisonError, Weak};

use natamux_git::gh::{GitHubError, RunGh, read_pull_request_status};
use natamux_proto::tasks::task::{Task, TaskState};

use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::scheduler::{Scheduler, TimerId};
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;

#[path = "pull_request_finder.rs"]
mod finder;

pub const WATCH_INTERVAL_MS: u64 = 60_000;
/// How often GitHub is asked once no watched pull request waits for its checks. Failures in a row
/// slow the checks down to this too.
pub const SETTLED_INTERVAL_MS: u64 = 5 * 60_000;

/// An agent usually opens its pull request just before its turn ends.
const AGENT_STOP_EVENTS: [&str; 4] = [
    "agent.finished",
    "agent.waiting",
    "agent.stopped",
    "stage.reported",
];

/// Where gh runs for the tasks of a repository. A full pull request address works from any folder.
pub type CwdOf = Arc<dyn Fn(Option<&str>) -> String + Send + Sync>;
/// Whether origin has the branch of a worktree: no network.
pub type HasRemoteBranch = Arc<dyn Fn(&Path, &str) -> Result<bool, String> + Send + Sync>;
pub type Log = Arc<dyn Fn(&str) + Send + Sync>;
/// The task, after its branch follows the one checked out in its worktree.
pub type SyncBranch = Arc<dyn Fn(&str) -> Result<Task, TaskError> + Send + Sync>;

#[derive(Debug)]
pub enum WatchError {
    GitHub(GitHubError),
    Task(TaskError),
    /// git could not tell whether origin has the branch.
    Git(String),
}

impl std::fmt::Display for WatchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WatchError::GitHub(error) => write!(f, "{error}"),
            WatchError::Task(error) => write!(f, "{error}"),
            WatchError::Git(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for WatchError {}

impl From<GitHubError> for WatchError {
    fn from(error: GitHubError) -> WatchError {
        WatchError::GitHub(error)
    }
}

impl From<TaskError> for WatchError {
    fn from(error: TaskError) -> WatchError {
        WatchError::Task(error)
    }
}

/// A task is watched while it has an open pull request and is not cancelled.
pub fn is_watched(task: &Task) -> bool {
    if task.pr.is_none() || matches!(task.state, TaskState::Cancelled) {
        return false;
    }
    task.pr_status
        .as_ref()
        .is_none_or(|status| status.state == "open" || status.state == "draft")
}

/// The workspace and branch of a task whose pull request may have been opened outside Natamux, by
/// its agent or on GitHub.
fn candidate(task: &Task) -> Option<(&str, &str)> {
    let active = !matches!(task.state, TaskState::Cancelled | TaskState::Done);
    match (&task.pr, &task.workspace, &task.branch) {
        (None, Some(workspace), Some(branch)) if active => {
            Some((workspace.as_str(), branch.as_str()))
        }
        _ => None,
    }
}

pub fn is_candidate(task: &Task) -> bool {
    candidate(task).is_some()
}

/// Settled: the status is known and no check runs, so it changes only by people.
pub fn is_settled(task: &Task) -> bool {
    task.pr_status
        .as_ref()
        .is_some_and(|status| status.checks != "pending")
}

pub struct PullRequestWatcherOptions {
    pub tasks: TaskService,
    pub cwd_of: CwdOf,
    pub gh: RunGh,
    pub scheduler: Arc<Scheduler>,
    pub interval_ms: u64,
    /// Default: a line on stderr.
    pub log: Option<Log>,
    /// Only dashboards show the status, so the timed checks rest while none listens. Without a bus
    /// they always run.
    pub bus: Option<Arc<EventBus>>,
    /// Default: the remote-tracking ref of `natamux_git::graph::has_remote_branch`.
    pub has_remote_branch: Option<HasRemoteBranch>,
    /// Asked before a task's pull request is looked for. Default: the task as stored.
    pub sync_branch: SyncBranch,
}

impl PullRequestWatcherOptions {
    pub fn new(
        tasks: TaskService,
        cwd_of: CwdOf,
        gh: RunGh,
        scheduler: Arc<Scheduler>,
    ) -> PullRequestWatcherOptions {
        let reading = tasks.clone();
        PullRequestWatcherOptions {
            tasks,
            cwd_of,
            gh,
            scheduler,
            interval_ms: WATCH_INTERVAL_MS,
            log: None,
            bus: None,
            has_remote_branch: None,
            sync_branch: Arc::new(move |task_id| reading.get_task(task_id)),
        }
    }
}

enum Work {
    CheckAll,
    Check(String),
}

#[derive(Default)]
struct State {
    started: bool,
    /// Tells a timer of an earlier `start` from the current one.
    generation: u64,
    timer: Option<TimerId>,
    subscription: Option<u64>,
    sender: Option<Sender<Work>>,
    /// Tasks whose pull request is being looked for now.
    finding: HashSet<String>,
    /// Tasks without a pull request whose branch origin had at the last look. Kept here because
    /// `due` cannot ask git.
    on_origin: HashSet<String>,
    running: bool,
    /// A `CheckAll` is on the channel.
    queued: bool,
    last_error: Option<String>,
    last_check_at: Option<u64>,
    /// Checks in a row that had a failure.
    failures: u32,
}

struct Inner {
    options: PullRequestWatcherOptions,
    has_remote_branch: HasRemoteBranch,
    state: Mutex<State>,
    /// Work on the channel or running, for `settle`.
    pending: Mutex<usize>,
    idle: Condvar,
}

pub struct PullRequestWatcher {
    inner: Arc<Inner>,
}

fn locked<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl PullRequestWatcher {
    pub fn new(options: PullRequestWatcherOptions) -> PullRequestWatcher {
        let has_remote_branch = options.has_remote_branch.clone().unwrap_or_else(|| {
            Arc::new(|worktree: &Path, branch: &str| {
                natamux_git::graph::has_remote_branch(worktree, branch)
                    .map_err(|error| error.to_string())
            })
        });
        PullRequestWatcher {
            inner: Arc::new(Inner {
                options,
                has_remote_branch,
                state: Mutex::new(State::default()),
                pending: Mutex::new(0),
                idle: Condvar::new(),
            }),
        }
    }

    /// The timer looks every `interval_ms`; whether it asks GitHub depends on the dashboards, the
    /// checks and the failures (see `due`). Does nothing when it runs already.
    pub fn start(&self) {
        let inner = &self.inner;
        let mut spawn_error = None;
        {
            let mut state = locked(&inner.state);
            if state.started {
                return;
            }
            state.started = true;
            state.generation += 1;
            let (sender, receiver) = channel();
            state.sender = Some(sender);
            let spawned = std::thread::Builder::new().name("pr-watch".into()).spawn({
                let weak = Arc::downgrade(inner);
                move || work_loop(&weak, &receiver)
            });
            if let Err(error) = spawned {
                spawn_error = Some(format!("could not start the watcher thread: {error}"));
            }
            if let Some(bus) = &inner.options.bus {
                let weak = Arc::downgrade(inner);
                state.subscription = Some(bus.subscribe(move |message| {
                    if let Some(inner) = weak.upgrade() {
                        inner.on_message(message);
                    }
                }));
            }
            let generation = state.generation;
            state.timer = Some(inner.arm(generation));
        }
        if let Some(text) = spawn_error {
            inner.report(&text);
        }
        if inner.has_clients() {
            inner.request_check_all();
        }
    }

    pub fn stop(&self) {
        let inner = &self.inner;
        let mut state = locked(&inner.state);
        state.started = false;
        state.generation += 1;
        inner.options.scheduler.clear(&mut state.timer);
        if let (Some(bus), Some(id)) = (&inner.options.bus, state.subscription.take()) {
            bus.unsubscribe(id);
        }
        // The worker ends after the work already on the channel.
        state.sender = None;
    }

    /// A dashboard came while none listened: it gets fresh statuses, unless a check ran just now.
    /// Returns at once; the worker thread asks GitHub.
    pub fn wake(&self) {
        let inner = &self.inner;
        let now = inner.options.scheduler.now();
        let due = {
            let state = locked(&inner.state);
            state.started
                && state
                    .last_check_at
                    .is_none_or(|at| now.saturating_sub(at) >= inner.options.interval_ms)
        };
        if due {
            inner.request_check_all();
        }
    }

    /// Checks every watched task, on the caller's thread. A failure for one repository does not
    /// stop the others. Does nothing while another check runs.
    pub fn check_all(&self) {
        self.inner.check_all();
    }

    pub fn check(&self, task_id: &str) -> Result<Task, WatchError> {
        self.inner.check(task_id)
    }

    /// Like the auto fetch of VS Code: slower when nothing is about to change, and after failures.
    pub fn due(&self) -> bool {
        self.inner.due()
    }

    /// Waits for the work the worker thread has now. For tests.
    pub fn settle(&self) {
        let mut pending = locked(&self.inner.pending);
        while *pending > 0 {
            pending = self
                .inner
                .idle
                .wait(pending)
                .unwrap_or_else(PoisonError::into_inner);
        }
    }
}

fn work_loop(weak: &Weak<Inner>, receiver: &Receiver<Work>) {
    while let Ok(work) = receiver.recv() {
        let Some(inner) = weak.upgrade() else {
            return;
        };
        match work {
            Work::CheckAll => {
                locked(&inner.state).queued = false;
                inner.check_all();
            }
            Work::Check(task_id) => {
                if let Err(error) = inner.check(&task_id) {
                    inner.report(&error.to_string());
                }
            }
        }
        inner.finish_one();
    }
}

impl Inner {
    fn now(&self) -> u64 {
        self.options.scheduler.now()
    }

    fn has_clients(&self) -> bool {
        self.options.bus.as_ref().is_none_or(|bus| bus.size() > 0)
    }

    /// The next timer tick, like `setInterval`.
    fn arm(self: &Arc<Inner>, generation: u64) -> TimerId {
        let weak = Arc::downgrade(self);
        self.options
            .scheduler
            .after(self.options.interval_ms, move || {
                if let Some(inner) = weak.upgrade() {
                    inner.tick(generation);
                }
            })
    }

    fn tick(self: &Arc<Inner>, generation: u64) {
        {
            let mut state = locked(&self.state);
            if !state.started || state.generation != generation {
                return;
            }
            state.timer = Some(self.arm(generation));
        }
        if self.due() {
            self.request_check_all();
        }
    }

    /// Puts a check of everything on the channel, unless one runs or waits there.
    fn request_check_all(&self) {
        {
            let mut state = locked(&self.state);
            if state.queued || state.running {
                return;
            }
            state.queued = true;
            // Set now, not when the worker starts: the next tick must see this check.
            state.last_check_at = Some(self.now());
        }
        if !self.send(Work::CheckAll) {
            locked(&self.state).queued = false;
        }
    }

    fn send(&self, work: Work) -> bool {
        let Some(sender) = locked(&self.state).sender.clone() else {
            return false;
        };
        *locked(&self.pending) += 1;
        if sender.send(work).is_err() {
            self.finish_one();
            return false;
        }
        true
    }

    fn finish_one(&self) {
        let mut pending = locked(&self.pending);
        *pending = pending.saturating_sub(1);
        self.idle.notify_all();
    }

    fn on_message(&self, message: &DaemonMessage) {
        if let DaemonMessage::TaskUpdated { task, event, .. } = message
            && AGENT_STOP_EVENTS.contains(&event.kind.as_str())
            && is_candidate(task)
        {
            self.send(Work::Check(task.id.clone()));
        }
    }

    fn due(&self) -> bool {
        if !self.has_clients() {
            return false;
        }
        let (on_origin, failures, last_check_at) = {
            let state = locked(&self.state);
            (state.on_origin.clone(), state.failures, state.last_check_at)
        };
        let tasks = self.options.tasks.list_tasks(None).unwrap_or_default();
        // A pushed branch without a pull request may get one any minute.
        let pushed = tasks
            .iter()
            .any(|task| is_candidate(task) && on_origin.contains(&task.id));
        let settled = !pushed && tasks.iter().filter(|task| is_watched(task)).all(is_settled);
        let interval = self.options.interval_ms;
        let base = if settled {
            SETTLED_INTERVAL_MS
        } else {
            interval
        };
        let wait = base
            .saturating_mul(2u64.saturating_pow(failures))
            .min(SETTLED_INTERVAL_MS);
        let elapsed = last_check_at.map_or(u64::MAX, |at| self.now().saturating_sub(at));
        // Half a tick of slack: by the wall clock, a tick may come a moment before the full wait.
        elapsed >= wait.saturating_sub(interval / 2)
    }

    /// The same failure every minute (gh missing, not logged in) is said once.
    fn report(&self, message: &str) {
        {
            let mut state = locked(&self.state);
            if state.last_error.as_deref() == Some(message) {
                return;
            }
            state.last_error = Some(message.to_string());
        }
        let line = format!("Pull request check: {message}");
        match &self.options.log {
            Some(log) => log(&line),
            None => eprintln!("{line}"),
        }
    }

    fn fail(&self, failed: &mut bool, error: &WatchError) {
        *failed = true;
        self.report(&error.to_string());
    }

    fn check_all(&self) {
        {
            let mut state = locked(&self.state);
            if state.running {
                return;
            }
            state.running = true;
            state.last_check_at = Some(self.now());
        }
        let mut failed = false;
        // The batches below read the status of a pull request found here.
        self.find_all(&mut failed);
        self.read_all(&mut failed);
        let mut state = locked(&self.state);
        state.failures = if failed { state.failures + 1 } else { 0 };
        state.running = false;
    }

    fn check(&self, task_id: &str) -> Result<Task, WatchError> {
        let tasks = &self.options.tasks;
        if tasks.get_task(task_id)?.pr.is_none() {
            self.find_one(task_id)?;
        }
        let task = tasks.get_task(task_id)?;
        let Some(url) = task.pr.as_deref() else {
            return Ok(task);
        };
        let folder = (self.options.cwd_of)(task.repo.as_deref());
        let status = read_pull_request_status(Path::new(&folder), url, &self.options.gh)?;
        Ok(tasks.record_pull_request_status(task_id, status)?)
    }
}
