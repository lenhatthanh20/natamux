//! The dashboard data as the live feed brings it, with no gpui: each part is replaced whole. The
//! entity that holds it is `app/dashboard.rs`.

use std::cell::OnceCell;
use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};
use std::sync::Arc;
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
use crate::tasks::table_rules::{OpenSubtasks, open_subtasks, waits_for_in};
use crate::tasks::task_changes::upsert_task_in;

#[path = "dashboard_parts.rs"]
mod parts;
pub use parts::{Changed, Generations, Part, Seen};

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

/// The bulk runs that work one task after the other (`removingFinished` of the web). A second run
/// while one goes would race its git work on the same ids and fail on each task, so it is
/// refused, and its buttons are off. Deletes go through `DeleteQueue` instead.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BulkRun {
    RemoveFinished,
}

#[derive(Debug, Default)]
pub struct BulkRuns {
    removing_finished: bool,
}

impl BulkRuns {
    pub fn running(&self, run: BulkRun) -> bool {
        match run {
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
    /// Shared: a view that keeps a task keeps the Arc, and a task that did not change keeps its
    /// Arc, so an equal task is found equal by its address. Changed only by the methods here,
    /// which keep `by_id` in step.
    pub tasks: Vec<Arc<Task>>,
    /// The tasks of `tasks` by id, so `task` is one lookup and not a scan of the list.
    by_id: HashMap<String, Arc<Task>>,
    /// The open subtasks of each parent, made on the first ask after the list changed: a row
    /// asks what its task waits for on each draw.
    open_subtasks: OnceCell<OpenSubtasks>,
    /// Tasks hidden while their delete runs, with their newest state. The daemon writes a task
    /// while it deletes it, and the feed must not show it again.
    deleting: BTreeMap<String, Arc<Task>>,
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
    /// The Tasks generation after the last task event that changed the list: the followers of
    /// the list were told up to it (`tasks_news`).
    pub(super) tasks_told: u64,
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
            by_id: HashMap::new(),
            open_subtasks: OnceCell::new(),
            deleting: BTreeMap::new(),
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
            tasks_told: 0,
        }
    }
}

impl DashboardData {
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.by_id.get(id).map(|task| &**task)
    }

    /// `task`, shared.
    #[cfg(test)]
    pub fn shared_task(&self, id: &str) -> Option<&Arc<Task>> {
        self.by_id.get(id)
    }

    /// The tasks `task` waits for (`table_rules::waits_for`), with no read of the whole list.
    pub fn waits_for(&self, task: &Task) -> Vec<String> {
        let open = &self.open_subtasks;
        let open = open.get_or_init(|| open_subtasks(&self.tasks));
        waits_for_in(task, |id| self.task(id).map(|other| other.state), open)
    }

    /// Takes one event of the feed. Gives a toast when a new workspace of this app ended.
    pub fn apply(&mut self, event: DashboardEvent) -> Option<Notice> {
        let changes = self.changes(&event);
        if changes {
            self.bump_parts(&event);
        }
        match event {
            DashboardEvent::Tasks(tasks) => {
                // An equal list has no task that is being deleted, and the tasks held stay.
                if changes {
                    self.set_tasks(tasks);
                    self.tasks_told = self.gens.get(Part::Tasks);
                }
                self.loaded = true;
                self.link = Link::Live;
                let known = &self.by_id;
                self.events.retain(|id, _| known.contains_key(id.as_str()));
                self.terminals
                    .retain(|id, _| known.contains_key(id.as_str()));
                crate::perf::report::mark_first_data();
            }
            // No task goes, so nothing tied to one is dropped; one task is not the loaded list.
            DashboardEvent::TaskChanged(task) => {
                let before = self.gens.get(Part::Tasks);
                self.merge_task(*task);
                // Only a change is told: an equal task leaves an untold answer untold.
                if self.gens.get(Part::Tasks) != before {
                    self.tasks_told = self.gens.get(Part::Tasks);
                }
            }
            DashboardEvent::Repos {
                repos,
                worktrees_dir,
            } => {
                self.repos = repos;
                self.worktrees_dir = worktrees_dir;
            }
            DashboardEvent::Jobs(jobs) => self.jobs = jobs,
            DashboardEvent::Activity(activity) => self.activity = activity,
            // As `GET /activity` gives it: a task with no line is left out.
            DashboardEvent::ActivityOf { task_id, activity } => {
                if activity.is_empty() {
                    self.activity.remove(&task_id);
                } else {
                    self.activity.insert(task_id, activity);
                }
            }
            DashboardEvent::Usage(usage) => self.usage = usage,
            DashboardEvent::UsageOf { task_id, usage } => {
                self.usage.insert(task_id, usage);
            }
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

    /// Takes a whole list. A task equal to the one held keeps the Arc held, so the rows that
    /// show it are found unchanged without a look inside.
    fn set_tasks(&mut self, tasks: Vec<Task>) {
        let mut held = std::mem::take(&mut self.by_id);
        let mut by_id = HashMap::with_capacity(tasks.len());
        let mut list = Vec::with_capacity(tasks.len());
        for task in tasks {
            if let Some(copy) = self.deleting.get_mut(&task.id) {
                *copy = Arc::new(task);
                continue;
            }
            let (id, before) = match held.remove_entry(&task.id) {
                Some((id, before)) => (id, Some(before)),
                None => (task.id.clone(), None),
            };
            let task = match before {
                Some(before) if *before == task => before,
                _ => Arc::new(task),
            };
            // The first task with an id wins, as a search from the start would find it.
            by_id.entry(id).or_insert_with(|| task.clone());
            list.push(task);
        }
        self.tasks = list;
        self.by_id = by_id;
        self.open_subtasks = OnceCell::new();
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

    /// This app asked for the worktree of `id`: its first step shows at once, since the daemon's
    /// step event can come after the answer. The daemon's steps and end take over.
    /// shortcut: a new task whose create fails before its first step gets no end event and keeps
    /// the spinner until the app connects again; the daemon never fails there for a new task.
    pub fn expect_preparing(&mut self, id: &str) {
        if !self.preparing.contains_key(id) {
            self.preparing.insert(id.to_owned(), PreparingStep::Fetch);
            self.gens.bump(Part::Preparing);
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
        if let Some(copy) = self.deleting.get_mut(&task.id) {
            if copy.updated_at <= task.updated_at {
                *copy = Arc::new(task);
            }
            return;
        }
        if self.task_changes(&task) {
            self.gens.bump(Part::Tasks);
        }
        // An equal task keeps the Arc held: only its place in the list is checked again.
        let task = match self.by_id.get(&task.id) {
            Some(held) if **held == task => held.clone(),
            _ => Arc::new(task),
        };
        self.put(task);
    }

    /// The task into its place in the list, unless the one held is newer.
    fn put(&mut self, task: Arc<Task>) {
        if !upsert_task_in(&mut self.tasks, task.clone()) {
            return;
        }
        self.open_subtasks = OnceCell::new();
        match self.by_id.get_mut(&task.id) {
            Some(held) => *held = task,
            None => {
                self.by_id.insert(task.id.clone(), task);
            }
        }
    }

    pub fn is_deleting(&self, id: &str) -> bool {
        self.deleting.contains_key(id)
    }

    /// A delete started: the task leaves the views at once, with what belongs to it.
    pub fn hide_task(&mut self, id: &str) {
        let Some(at) = self.tasks.iter().position(|task| task.id == id) else {
            return;
        };
        let task = self.tasks.remove(at);
        self.by_id.remove(id);
        self.open_subtasks = OnceCell::new();
        self.deleting.insert(task.id.clone(), task);
        self.drop_parts_of(id);
    }

    /// The delete failed: the task shows again, as the feed last brought it.
    pub fn restore_task(&mut self, id: &str) {
        if let Some(task) = self.deleting.remove(id) {
            self.put(task);
            self.gens.bump(Part::Tasks);
        }
    }

    pub fn forget_task(&mut self, id: &str) {
        self.deleting.remove(id);
        self.by_id.remove(id);
        self.open_subtasks = OnceCell::new();
        self.tasks.retain(|task| task.id != id);
        self.drop_parts_of(id);
    }

    fn drop_parts_of(&mut self, id: &str) {
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
