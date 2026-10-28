//! The dashboard data as the live feed brings it, with no gpui: each part is replaced whole. The
//! entity that holds it is `app/dashboard.rs`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::time::Instant;

use natamux_client::{ClientError, DashboardEvent, LinkStatus};
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::agents::limits::AgentLimits;
use natamux_proto::events::kind::PreparingMessage;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{DiffStat, PreparingStep};

use crate::agents::limits_rules::{new_warning, warned_already};
use crate::components::toast_rules::ToastTone;
use crate::sidebar::sidebar_tree::{RowTier, row_tier};
use crate::tasks::task_changes::upsert_task;

#[path = "dashboard_parts.rs"]
mod parts;
pub use parts::{Generations, Part, Seen};

/// What the app knows of the daemon.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Link {
    Loading,
    Live,
    /// The daemon could not be used, and why. `retrying`: the client tries again by itself.
    Down {
        reason: String,
        retrying: bool,
    },
}

/// The bulk runs that work one task after the other (`deletingTasks` and `removingFinished` of
/// the web). A second run while one goes would race its git work on the same ids and fail on each
/// task, so it is refused, and its buttons are off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkRun {
    DeleteTasks,
    RemoveFinished,
}

#[derive(Debug, Default)]
pub struct BulkRuns {
    deleting_tasks: bool,
    removing_finished: bool,
}

impl BulkRuns {
    pub fn running(&self, run: BulkRun) -> bool {
        match run {
            BulkRun::DeleteTasks => self.deleting_tasks,
            BulkRun::RemoveFinished => self.removing_finished,
        }
    }

    /// Starts `run`. False when it runs already: the caller does nothing.
    pub fn start(&mut self, run: BulkRun) -> bool {
        !std::mem::replace(self.flag(run), true)
    }

    pub fn finish(&mut self, run: BulkRun) {
        *self.flag(run) = false;
    }

    fn flag(&mut self, run: BulkRun) -> &mut bool {
        match run {
            BulkRun::DeleteTasks => &mut self.deleting_tasks,
            BulkRun::RemoveFinished => &mut self.removing_finished,
        }
    }
}

/// A toast the data asks for: the end of a new workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub text: String,
    pub tone: ToastTone,
}

#[derive(Debug)]
pub struct DashboardData {
    pub link: Link,
    /// When the link went down, for the `app.log` line once it is back.
    pub link_down: Option<Instant>,
    /// The daemon refused the token: the window asks for one (Sign in).
    pub refused: bool,
    /// The task list came at least once: before that the views show skeletons.
    pub loaded: bool,
    pub tasks: Vec<Task>,
    pub repos: Vec<Repo>,
    pub worktrees_dir: Option<String>,
    pub jobs: Vec<Job>,
    pub activity: BTreeMap<String, TaskActivity>,
    pub usage: BTreeMap<String, TaskUsage>,
    /// The plan limits of the agents that told them.
    pub limits: Vec<AgentLimits>,
    /// The windows the warning toast told already; None until the first limits came.
    warned: Option<BTreeSet<String>>,
    pub diff_stats: BTreeMap<String, DiffStat>,
    pub terminals: BTreeMap<String, TerminalList>,
    pub preparing: BTreeMap<String, PreparingStep>,
    /// The event log of each task that shows an attention card, with the `updatedAt` it was
    /// read for: the question or the failure reason.
    pub events: BTreeMap<String, (String, Vec<StoredEvent>)>,
    /// New workspaces of this app: a toast says when each is ready (`announceWorkspace`).
    announced: BTreeSet<String>,
    /// Workspaces that ended before the app announced them, with their error, newest last. Most
    /// were made elsewhere (web, CLI) and are never announced, so only the last few are kept.
    ended_early: VecDeque<(String, Option<String>)>,
    /// What changed: views that follow the dashboard skip a notify of parts they do not read.
    pub gens: Generations,
}

/// A POST /workspaces answer comes within a few workspaces' ends, so this many is plenty.
const ENDED_KEPT: usize = 32;

impl Default for DashboardData {
    fn default() -> Self {
        DashboardData {
            link: Link::Loading,
            link_down: None,
            refused: false,
            loaded: false,
            tasks: Vec::new(),
            repos: Vec::new(),
            worktrees_dir: None,
            jobs: Vec::new(),
            activity: BTreeMap::new(),
            usage: BTreeMap::new(),
            limits: Vec::new(),
            warned: None,
            diff_stats: BTreeMap::new(),
            terminals: BTreeMap::new(),
            preparing: BTreeMap::new(),
            events: BTreeMap::new(),
            announced: BTreeSet::new(),
            ended_early: VecDeque::new(),
            gens: Generations::default(),
        }
    }
}

impl DashboardData {
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// Takes one event of the feed. Gives a toast when a new workspace of this app ended.
    pub fn apply(&mut self, event: DashboardEvent) -> Option<Notice> {
        if self.changes(&event) {
            self.bump_parts(&event);
        }
        match event {
            DashboardEvent::Tasks(tasks) => {
                self.tasks = tasks;
                self.loaded = true;
                self.link = Link::Live;
                let known: BTreeSet<&str> =
                    self.tasks.iter().map(|task| task.id.as_str()).collect();
                self.events.retain(|id, _| known.contains(id.as_str()));
                self.terminals.retain(|id, _| known.contains(id.as_str()));
                crate::perf::report::mark_first_data();
            }
            // No task goes, so nothing tied to one is dropped; one task is not the loaded list.
            DashboardEvent::TaskChanged(task) => self.merge_task(*task),
            DashboardEvent::Repos {
                repos,
                worktrees_dir,
            } => {
                self.repos = repos;
                self.worktrees_dir = worktrees_dir;
            }
            DashboardEvent::Jobs(jobs) => self.jobs = jobs,
            DashboardEvent::Activity(activity) => self.activity = activity,
            DashboardEvent::Usage(usage) => self.usage = usage,
            DashboardEvent::Limits(limits) => return self.set_limits(limits),
            DashboardEvent::DiffStats(stats) => self.diff_stats = stats,
            DashboardEvent::Terminals { task_id, terminals } => {
                self.terminals.insert(task_id, terminals);
            }
            DashboardEvent::Preparing(preparing) => self.preparing = preparing,
            DashboardEvent::WorkspaceStep(message) => return self.step(message),
            DashboardEvent::Status(status) => self.link_status(status),
            // The open workspace reads what it shows (`app/dashboard.rs`).
            DashboardEvent::WorkspaceChanged { .. }
            | DashboardEvent::ShellsChanged { .. }
            | DashboardEvent::ReviewCommentsChanged { .. }
            | DashboardEvent::TaskLogged { .. } => {}
        }
        None
    }

    fn link_status(&mut self, status: LinkStatus) {
        self.link = match status {
            LinkStatus::Connecting => return,
            LinkStatus::Live => {
                self.refused = false;
                Link::Live
            }
            LinkStatus::Reconnecting(error) => Link::Down {
                reason: error.to_string(),
                retrying: true,
            },
            LinkStatus::Refused => {
                self.refused = true;
                Link::Down {
                    reason: ClientError::Unauthorized.to_string(),
                    retrying: false,
                }
            }
        };
    }

    /// The warning toast when a window passed the warning share; the limits the app saw first
    /// are not news.
    fn set_limits(&mut self, limits: Vec<AgentLimits>) -> Option<Notice> {
        self.limits = limits;
        let Some(warned) = &mut self.warned else {
            self.warned = Some(warned_already(&self.limits));
            return None;
        };
        new_warning(warned, &self.limits).map(|text| Notice {
            text,
            tone: ToastTone::Info,
        })
    }

    fn step(&mut self, message: PreparingMessage) -> Option<Notice> {
        let id = message.task_id;
        if let Some(step) = message.step {
            self.preparing.insert(id, step);
            return None;
        }
        self.preparing.remove(&id);
        if !self.announced.remove(&id) {
            self.ended_early.retain(|(each, _)| *each != id);
            self.ended_early.push_back((id, message.error));
            if self.ended_early.len() > ENDED_KEPT {
                self.ended_early.pop_front();
            }
            return None;
        }
        Some(self.prepared(&id, message.error))
    }

    /// `onPrepared`: the ready or the failure toast.
    pub fn prepared(&self, id: &str, error: Option<String>) -> Notice {
        match error {
            Some(error) => Notice {
                text: format!("{id}: workspace failed. {error}"),
                tone: ToastTone::Error,
            },
            None => {
                let branch = self.task(id).and_then(|task| task.branch.as_deref());
                Notice {
                    text: format!("{id} is ready in {}.", branch.unwrap_or("its branch")),
                    tone: ToastTone::Info,
                }
            }
        }
    }

    /// The app made this workspace: a toast says when it is ready. A workspace that ended
    /// already gives its toast now.
    pub fn announce(&mut self, id: &str) -> Option<Notice> {
        let ended = self.ended_early.iter().position(|(each, _)| each == id);
        match ended.and_then(|at| self.ended_early.remove(at)) {
            Some((_, error)) => Some(self.prepared(id, error)),
            None => {
                self.announced.insert(id.to_owned());
                None
            }
        }
    }

    /// Tasks with an attention card whose event log is older than the task: (id, updatedAt).
    pub fn events_to_read(&self) -> Vec<(String, String)> {
        self.tasks
            .iter()
            .filter(|task| row_tier(task.state) == RowTier::Attention)
            .filter(|task| {
                self.events
                    .get(&task.id)
                    .is_none_or(|(read_for, _)| *read_for != task.updated_at)
            })
            .map(|task| (task.id.clone(), task.updated_at.clone()))
            .collect()
    }

    pub fn set_events(&mut self, id: &str, updated_at: &str, events: Vec<StoredEvent>) {
        self.events
            .insert(id.to_owned(), (updated_at.to_owned(), events));
        self.gens.bump(Part::Events);
    }

    /// A task an answer of the daemon changed, before the feed brings the list. A late answer
    /// keeps what the feed brought since (`upsertTask`).
    pub fn merge_task(&mut self, task: Task) {
        if self.task_changes(&task) {
            self.gens.bump(Part::Tasks);
        }
        self.tasks = upsert_task(&self.tasks, task);
    }

    pub fn forget_task(&mut self, id: &str) {
        self.tasks.retain(|task| task.id != id);
        self.events.remove(id);
        self.terminals.remove(id);
        for part in [Part::Tasks, Part::Events, Part::Terminals] {
            self.gens.bump(part);
        }
    }
}

#[cfg(test)]
#[path = "dashboard_data_tests.rs"]
mod tests;
