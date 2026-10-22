//! The dashboard data as the live feed brings it, with no gpui: each part is replaced whole. The
//! entity that holds it is `app/dashboard.rs`.

use std::collections::{BTreeMap, BTreeSet, VecDeque};

use natamux_client::{ClientError, DashboardEvent, LinkStatus};
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::events::kind::PreparingMessage;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{DiffStat, PreparingStep};

use crate::components::toast_rules::ToastTone;
use crate::sidebar::sidebar_tree::{RowTier, row_tier};
use crate::tasks::task_changes::upsert_task;

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
    /// The task list came at least once: before that the views show skeletons.
    pub loaded: bool,
    pub tasks: Vec<Task>,
    pub repos: Vec<Repo>,
    pub worktrees_dir: Option<String>,
    pub jobs: Vec<Job>,
    pub activity: BTreeMap<String, TaskActivity>,
    pub usage: BTreeMap<String, TaskUsage>,
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
}

/// A POST /workspaces answer comes within a few workspaces' ends, so this many is plenty.
const ENDED_KEPT: usize = 32;

impl Default for DashboardData {
    fn default() -> Self {
        DashboardData {
            link: Link::Loading,
            loaded: false,
            tasks: Vec::new(),
            repos: Vec::new(),
            worktrees_dir: None,
            jobs: Vec::new(),
            activity: BTreeMap::new(),
            usage: BTreeMap::new(),
            diff_stats: BTreeMap::new(),
            terminals: BTreeMap::new(),
            preparing: BTreeMap::new(),
            events: BTreeMap::new(),
            announced: BTreeSet::new(),
            ended_early: VecDeque::new(),
        }
    }
}

impl DashboardData {
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    /// Takes one event of the feed. Gives a toast when a new workspace of this app ended.
    pub fn apply(&mut self, event: DashboardEvent) -> Option<Notice> {
        match event {
            DashboardEvent::Tasks(tasks) => {
                self.tasks = tasks;
                self.loaded = true;
                self.link = Link::Live;
                let known: BTreeSet<&str> =
                    self.tasks.iter().map(|task| task.id.as_str()).collect();
                self.events.retain(|id, _| known.contains(id.as_str()));
                self.terminals.retain(|id, _| known.contains(id.as_str()));
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
            DashboardEvent::Usage(usage) => self.usage = usage,
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
            | DashboardEvent::TaskLogged { .. } => {}
        }
        None
    }

    fn link_status(&mut self, status: LinkStatus) {
        self.link = match status {
            LinkStatus::Connecting => return,
            LinkStatus::Live => Link::Live,
            LinkStatus::Reconnecting(error) => Link::Down {
                reason: error.to_string(),
                retrying: true,
            },
            LinkStatus::Refused => Link::Down {
                reason: ClientError::Unauthorized.to_string(),
                retrying: false,
            },
        };
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
    }

    /// A task an answer of the daemon changed, before the feed brings the list. A late answer
    /// keeps what the feed brought since (`upsertTask`).
    pub fn merge_task(&mut self, task: Task) {
        self.tasks = upsert_task(&self.tasks, task);
    }

    pub fn forget_task(&mut self, id: &str) {
        self.tasks.retain(|task| task.id != id);
        self.events.remove(id);
        self.terminals.remove(id);
    }
}

#[cfg(test)]
mod tests {
    use natamux_proto::tasks::task::TaskState;

    use super::*;

    #[test]
    fn a_bulk_run_does_not_start_twice_while_it_runs() {
        let mut runs = BulkRuns::default();
        assert!(runs.start(BulkRun::RemoveFinished));
        assert!(runs.running(BulkRun::RemoveFinished));
        assert!(!runs.start(BulkRun::RemoveFinished));
        // The other kind is its own run.
        assert!(!runs.running(BulkRun::DeleteTasks));
        assert!(runs.start(BulkRun::DeleteTasks));
        runs.finish(BulkRun::RemoveFinished);
        assert!(!runs.running(BulkRun::RemoveFinished));
        assert!(runs.start(BulkRun::RemoveFinished));
    }

    fn task(id: &str, state: TaskState, updated_at: &str) -> Task {
        Task {
            id: id.into(),
            state,
            branch: Some(format!("natamux/{id}")),
            updated_at: updated_at.into(),
            ..Task::default()
        }
    }

    fn step(id: &str, step: Option<PreparingStep>, error: Option<&str>) -> DashboardEvent {
        DashboardEvent::WorkspaceStep(PreparingMessage {
            task_id: id.into(),
            step,
            error: error.map(String::from),
        })
    }

    #[test]
    fn the_task_list_makes_the_link_live_and_drops_what_belongs_to_gone_tasks() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Terminals {
            task_id: "T-2".into(),
            terminals: TerminalList::default(),
        });
        data.set_events("T-2", "a", Vec::new());
        assert!(!data.loaded);
        data.apply(DashboardEvent::Tasks(vec![task(
            "T-1",
            TaskState::Running,
            "a",
        )]));
        assert!(data.loaded);
        assert_eq!(data.link, Link::Live);
        assert!(data.terminals.is_empty());
        assert!(data.events.is_empty());
    }

    #[test]
    fn says_why_the_daemon_is_down_and_whether_it_tries_again() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Status(LinkStatus::Connecting));
        assert_eq!(data.link, Link::Loading);
        data.apply(DashboardEvent::Status(LinkStatus::Refused));
        assert!(matches!(
            data.link,
            Link::Down {
                retrying: false,
                ..
            }
        ));
        data.apply(DashboardEvent::Status(LinkStatus::Reconnecting(
            ClientError::NoToken,
        )));
        assert!(matches!(data.link, Link::Down { retrying: true, .. }));
    }

    #[test]
    fn toasts_the_end_of_an_announced_workspace_only() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Tasks(vec![task(
            "T-1",
            TaskState::Running,
            "a",
        )]));
        assert_eq!(data.announce("T-1"), None);
        assert_eq!(
            data.apply(step("T-1", Some(PreparingStep::Fetch), None)),
            None
        );
        assert_eq!(data.preparing.get("T-1"), Some(&PreparingStep::Fetch));
        let ready = data.apply(step("T-1", None, None));
        assert_eq!(
            ready.map(|notice| notice.text),
            Some("T-1 is ready in natamux/T-1.".into())
        );
        assert!(data.preparing.is_empty());
        assert_eq!(data.apply(step("T-9", None, None)), None);
    }

    #[test]
    fn toasts_a_workspace_that_ended_before_it_was_announced() {
        let mut data = DashboardData::default();
        data.apply(step("T-3", None, Some("fetch failed")));
        let failed = data.announce("T-3");
        assert_eq!(
            failed,
            Some(Notice {
                text: "T-3: workspace failed. fetch failed".into(),
                tone: ToastTone::Error,
            })
        );
        assert_eq!(data.announce("T-3"), None);
    }

    #[test]
    fn keeps_only_the_last_workspaces_that_ended_unannounced() {
        let mut data = DashboardData::default();
        for n in 0..ENDED_KEPT + 8 {
            data.apply(step(&format!("T-{n}"), None, None));
        }
        assert_eq!(data.ended_early.len(), ENDED_KEPT);
        assert_eq!(data.announce("T-0"), None);
        let newest = format!("T-{}", ENDED_KEPT + 7);
        assert!(data.announce(&newest).is_some());
    }

    #[test]
    fn reads_the_events_of_attention_cards_again_when_the_task_changed() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Tasks(vec![
            task("T-1", TaskState::NeedsInput, "a"),
            task("T-2", TaskState::Running, "a"),
            task("T-3", TaskState::Failed, "a"),
        ]));
        assert_eq!(
            data.events_to_read(),
            [("T-1".into(), "a".into()), ("T-3".into(), "a".into())]
        );
        data.set_events("T-1", "a", Vec::new());
        data.set_events("T-3", "old", Vec::new());
        assert_eq!(data.events_to_read(), [("T-3".into(), "a".into())]);
    }

    #[test]
    fn merges_an_answer_and_forgets_a_deleted_task() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Tasks(vec![task(
            "T-1",
            TaskState::Draft,
            "a",
        )]));
        data.merge_task(task("T-1", TaskState::Ready, "b"));
        data.merge_task(task("T-2", TaskState::Draft, "c"));
        assert_eq!(data.tasks.len(), 2);
        assert_eq!(data.tasks[0].id, "T-2");
        assert_eq!(
            data.task("T-1").map(|task| task.state),
            Some(TaskState::Ready)
        );
        data.forget_task("T-1");
        assert!(data.task("T-1").is_none());
    }

    #[test]
    fn a_late_answer_keeps_the_task_the_feed_brought_since() {
        let mut data = DashboardData::default();
        data.apply(DashboardEvent::Tasks(vec![task(
            "T-1",
            TaskState::Running,
            "2026-10-08T10:00:00.000Z",
        )]));
        data.merge_task(task("T-1", TaskState::Draft, "2026-10-08T09:00:00.000Z"));
        assert_eq!(
            data.task("T-1").map(|task| task.state),
            Some(TaskState::Running)
        );
    }
}
