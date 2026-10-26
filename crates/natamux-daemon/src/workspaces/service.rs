//! `workspace-service.ts`: the worktree of each task, and the reads and changes of its files. The
//! parts are `impl WorkspaceService` blocks in the other files of this folder (`create`, `remove`,
//! `changes`, `publish`, `new_workspace`, `fan_out`).
//!
//! Everything here blocks: callers run it on a thread of their own (a route uses `spawn_blocking`),
//! never on the HTTP reactor or the terminal output path. `touch` is the one call that is safe
//! there: it only marks and plans a timer.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};

use natamux_agents::launch::default_program;
use natamux_core::js::js_trim;
use natamux_git::branches::{list_branches, list_remote_branches};
use natamux_git::commit_drafter::AgentRun;
use natamux_git::gh::{RunGh, default_run_gh};
use natamux_proto::projects::repo::BranchList;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::workspace::DiffStat;

use crate::projects::repos::Repos;
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::{Clock, JobGuard, Jobs, system_clock};
use crate::shared::scheduler::Scheduler;
use crate::tasks::errors::TaskError;
use crate::tasks::service::TaskService;
use crate::workspaces::changes::WorkspaceChanges;
use crate::workspaces::diff_stats::DiffStats;
use crate::workspaces::dirty::DirtyTracker;
use crate::workspaces::errors::{WorkspaceError, WorkspaceResult};
use crate::workspaces::publish::DraftCell;

/// Ends the shells of a task and waits until they have ended.
pub type CloseTerminals = Arc<dyn Fn(&str) + Send + Sync>;
/// Called after a workspace is made, for example to run its setup script. Its failure is ignored.
pub type OnCreated = Arc<dyn Fn(&Task) + Send + Sync>;
/// The command of an agent (by its id), for the run that drafts a commit message.
pub type ProgramOf = Arc<dyn Fn(&str) -> String + Send + Sync>;
/// The body of a task's pull request. Its error (an issue that could not be opened) refuses the
/// pull request.
pub type PullRequestBody = Arc<dyn Fn(&Task) -> WorkspaceResult<String> + Send + Sync>;
/// Runs an agent once for a message: the output, or why it gave none.
pub type RunAgentOnce = Arc<dyn Fn(AgentRun) -> Result<String, String> + Send + Sync>;
/// Where a failed draft and the times of a new workspace are written down.
pub type LogFn = Arc<dyn Fn(&str) + Send + Sync>;

pub struct WorkspaceOptions {
    /// Where the stage files of a task are archived.
    pub data_dir: PathBuf,
    /// The folder of the worktrees of a project that has no folder of its own.
    pub worktrees_dir: PathBuf,
    pub tasks: TaskService,
    pub repos: Arc<Repos>,
    /// Dashboards hear when the files of a workspace may have changed, and the steps of a new one.
    pub bus: Arc<EventBus>,
    /// Each change of a workspace shows as a background job.
    pub jobs: Arc<Jobs>,
    /// The timers of `workspace.dirty`.
    pub scheduler: Arc<Scheduler>,
    /// For the time of each step of a new workspace, and the caches of git reads.
    pub now: Clock,
    pub close_terminals: CloseTerminals,
    pub on_created: Option<OnCreated>,
    pub program_of: ProgramOf,
    pub pull_request_body: PullRequestBody,
    pub run_gh: RunGh,
    pub run_agent_once: RunAgentOnce,
    pub log: LogFn,
}

impl WorkspaceOptions {
    /// The options with Node's defaults for the rest (`now` is the system clock). `run_agent_once`
    /// has no default here: the real one is in `workspaces::agent_once`, and the wiring sets it.
    pub fn new(
        data_dir: PathBuf,
        worktrees_dir: PathBuf,
        tasks: TaskService,
        repos: Arc<Repos>,
        bus: Arc<EventBus>,
        jobs: Arc<Jobs>,
        scheduler: Arc<Scheduler>,
    ) -> WorkspaceOptions {
        WorkspaceOptions {
            data_dir,
            worktrees_dir,
            tasks,
            repos,
            bus,
            jobs,
            scheduler,
            now: system_clock(),
            close_terminals: Arc::new(|_| {}),
            on_created: None,
            program_of: Arc::new(|agent| default_program(agent).to_string()),
            pull_request_body: Arc::new(|task| {
                let notes = js_trim(&task.description);
                Ok(if notes.is_empty() {
                    format!("Task {} in Natamux.", task.id)
                } else {
                    notes.to_string()
                })
            }),
            run_gh: default_run_gh(),
            run_agent_once: Arc::new(|_| Err("No agent runner is set".to_string())),
            log: Arc::new(|message| eprintln!("{message}")),
        }
    }
}

/// Work that goes on after its request answered; `settle` waits for it.
#[derive(Default)]
struct Background {
    running: Mutex<usize>,
    idle: Condvar,
}

struct BackgroundRun(Arc<Background>);

impl Drop for BackgroundRun {
    fn drop(&mut self) {
        let mut running = self
            .0
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *running -= 1;
        self.0.idle.notify_all();
    }
}

/// Frees the task for the next change and marks its workspace, also when the change panics.
struct Busy<'a> {
    service: &'a WorkspaceService,
    task_id: &'a str,
}

impl Drop for Busy<'_> {
    fn drop(&mut self) {
        self.service.lock_busy().remove(self.task_id);
        // Every change of a workspace runs here, so the next read sees it, even after a failure.
        self.service.touch(self.task_id);
    }
}

/// The busy slot and the job of a change started for an answer and finished on another thread
/// (a new workspace). Dropping it frees the task and marks its workspace, as `exclusive` does.
pub struct Claim {
    service: Arc<WorkspaceService>,
    task_id: String,
    _job: JobGuard,
}

impl Drop for Claim {
    fn drop(&mut self) {
        self.service.lock_busy().remove(&self.task_id);
        self.service.touch(&self.task_id);
    }
}

pub struct WorkspaceService {
    pub(crate) data_dir: PathBuf,
    pub(crate) worktrees_dir: PathBuf,
    pub(crate) tasks: TaskService,
    pub(crate) repos: Arc<Repos>,
    pub(crate) bus: Arc<EventBus>,
    pub(crate) jobs: Arc<Jobs>,
    pub(crate) now: Clock,
    pub(crate) close_terminals: CloseTerminals,
    pub(crate) on_created: Option<OnCreated>,
    pub(crate) program_of: ProgramOf,
    pub(crate) pull_request_body: PullRequestBody,
    pub(crate) run_gh: RunGh,
    pub(crate) run_agent_once: RunAgentOnce,
    pub(crate) log: LogFn,
    /// The tasks with a change that runs now.
    pub(crate) busy: Mutex<HashSet<String>>,
    /// The tasks whose workspace is being made, and the step each one is at, oldest first.
    pub(crate) steps: Mutex<Vec<(String, &'static str)>>,
    /// The commit message drafts that run now, one per task; a second ask waits for the same one.
    pub(crate) drafts: Mutex<HashMap<String, Arc<DraftCell>>>,
    pub(crate) stats: DiffStats,
    pub(crate) reads: WorkspaceChanges,
    pub(crate) dirty: DirtyTracker,
    background: Arc<Background>,
}

impl WorkspaceService {
    pub fn new(options: WorkspaceOptions) -> Arc<WorkspaceService> {
        let now = options.now;
        Arc::new(WorkspaceService {
            stats: DiffStats::new(
                options.tasks.clone(),
                Arc::clone(&options.repos),
                Arc::clone(&now),
            ),
            reads: WorkspaceChanges::new(Arc::clone(&now)),
            dirty: DirtyTracker::new(
                options.tasks.clone(),
                Some(Arc::clone(&options.bus)),
                options.scheduler,
            ),
            data_dir: options.data_dir,
            worktrees_dir: options.worktrees_dir,
            tasks: options.tasks,
            repos: options.repos,
            bus: options.bus,
            jobs: options.jobs,
            now,
            close_terminals: options.close_terminals,
            on_created: options.on_created,
            program_of: options.program_of,
            pull_request_body: options.pull_request_body,
            run_gh: options.run_gh,
            run_agent_once: options.run_agent_once,
            log: options.log,
            busy: Mutex::default(),
            steps: Mutex::default(),
            drafts: Mutex::default(),
            background: Arc::default(),
        })
    }

    /// The folder name is the task id only, to keep paths short.
    pub fn worktree_path_for(&self, task: &Task) -> PathBuf {
        self.repos
            .worktrees_dir_of(task.repo.as_deref())
            .unwrap_or_else(|| self.worktrees_dir.clone())
            .join(&task.id)
    }

    /// The repository folder of a task, or of a repository id (None: the default one).
    pub fn repo_dir_of(&self, repo: Option<&str>) -> Result<PathBuf, TaskError> {
        self.repos.path_of(repo).map(PathBuf::from)
    }

    /// The branches a workspace in that repository can start from.
    pub fn branches(&self, repo: Option<&str>) -> WorkspaceResult<BranchList> {
        Ok(list_branches(&self.repo_dir_of(repo)?)?)
    }

    /// The branches of origin, as `origin/<name>`, also the ones never fetched. Asks origin.
    pub fn remote_branches(&self, repo: Option<&str>) -> WorkspaceResult<Vec<String>> {
        Ok(list_remote_branches(&self.repo_dir_of(repo)?)?)
    }

    /// The folder of the task's workspace. `Conflict` when it has none.
    pub fn workspace_of(&self, task_id: &str) -> WorkspaceResult<PathBuf> {
        let task = self.tasks.get_task(task_id)?;
        task.workspace
            .map(PathBuf::from)
            .ok_or_else(|| WorkspaceError::Conflict(format!("{task_id} has no workspace")))
    }

    /// The tasks whose workspace is being made, and the step each one is at.
    pub fn preparing(&self) -> Vec<(String, &'static str)> {
        self.lock_steps().clone()
    }

    /// The size of the work in each workspace. A workspace that git cannot read is left out.
    pub fn diff_stats(&self) -> WorkspaceResult<Vec<(String, DiffStat)>> {
        self.stats.get()
    }

    /// The workspace of the task may have changed: by its agent, in one of its terminals, or here.
    /// The next reads of git see it, and dashboards hear of it (see `DIRTY_DEBOUNCE_MS`).
    pub fn touch(&self, task_id: &str) {
        self.stats.touch(task_id);
        self.reads.forget(task_id);
        self.dirty.touch(task_id);
    }

    /// Work that goes on after its request answered, such as a new workspace (see
    /// `new_workspace`). It runs on a thread of its own; a panic in it is not passed on.
    pub fn background(&self, work: impl FnOnce() + Send + 'static) {
        {
            let mut running = self
                .background
                .running
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            *running += 1;
        }
        let run = BackgroundRun(Arc::clone(&self.background));
        std::thread::spawn(move || {
            let _run = run;
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(work));
        });
    }

    /// `settle` with a deadline; false when the work was still running after `limit`.
    pub fn settle_for(&self, limit: std::time::Duration) -> bool {
        let running = self
            .background
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let (running, _) = self
            .background
            .idle
            .wait_timeout_while(running, limit, |running| *running > 0)
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *running == 0
    }

    /// For tests: waits until the work in the background is done.
    pub fn settle(&self) {
        let mut running = self
            .background
            .running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        while *running > 0 {
            running = self
                .background
                .idle
                .wait(running)
                .unwrap_or_else(|poisoned| poisoned.into_inner());
        }
    }

    /// One change per task: a second gets a conflict. The change shows as a job, and the task is
    /// touched when it ends, also after a failure.
    pub(crate) fn exclusive<T>(
        &self,
        task_id: &str,
        label: &str,
        work: impl FnOnce() -> WorkspaceResult<T>,
    ) -> WorkspaceResult<T> {
        if !self.lock_busy().insert(task_id.to_string()) {
            return Err(WorkspaceError::Conflict(format!(
                "Another workspace change for {task_id} is still running"
            )));
        }
        let _busy = Busy {
            service: self,
            task_id,
        };
        self.jobs
            .run_blocking("workspace", label, Some(task_id), work)
    }

    /// `exclusive` for a change that goes on in the background: the slot and the job are held
    /// until the claim is dropped.
    pub fn claim(self: &Arc<Self>, task_id: &str, label: &str) -> WorkspaceResult<Claim> {
        if !self.lock_busy().insert(task_id.to_string()) {
            return Err(WorkspaceError::Conflict(format!(
                "Another workspace change for {task_id} is still running"
            )));
        }
        Ok(Claim {
            service: Arc::clone(self),
            task_id: task_id.to_string(),
            _job: Jobs::start(&self.jobs, "workspace", label, Some(task_id)),
        })
    }

    fn lock_busy(&self) -> MutexGuard<'_, HashSet<String>> {
        self.busy
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn lock_steps(&self) -> MutexGuard<'_, Vec<(String, &'static str)>> {
        self.steps
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
