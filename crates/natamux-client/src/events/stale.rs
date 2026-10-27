//! What the dashboard data has out of date, from the messages of `/events`, and the one read per
//! resource that brings it back (`applyLiveMessages` in packages/web/src/lib/live.ts).

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use futures::channel::mpsc::UnboundedSender;
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::agents::limits::AgentLimits;
use natamux_proto::events::kind::{
    EventKind, PreparingMessage, Resource, is_task_updated, task_id_of,
};
use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::Task;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{DiffStat, PreparingStep};
use tokio::time::Instant;

use crate::config::Config;
use crate::error::ClientError;
use crate::events::live::LinkStatus;
use crate::{agents, projects, tasks, terminals, workspaces};

/// The dashboard data, whole each time one part of it is read again.
#[derive(Clone, Debug, PartialEq)]
pub enum DashboardEvent {
    Tasks(Vec<Task>),
    Repos {
        repos: Vec<Repo>,
        worktrees_dir: Option<String>,
    },
    Jobs(Vec<Job>),
    /// By task id.
    Activity(BTreeMap<String, TaskActivity>),
    /// By task id.
    Usage(BTreeMap<String, TaskUsage>),
    /// The plan limits of the agents that told them.
    Limits(Vec<AgentLimits>),
    /// By task id.
    DiffStats(BTreeMap<String, DiffStat>),
    /// The terminal rows of a task the app shows (`TerminalRows::show`).
    Terminals {
        task_id: String,
        terminals: TerminalList,
    },
    /// Every workspace being made, by task id, and its step.
    Preparing(BTreeMap<String, PreparingStep>),
    /// One step of a new workspace, as it comes. `step` None: it ended, ready or failed.
    WorkspaceStep(PreparingMessage),
    Status(LinkStatus),
    /// Files changed in a task's workspace. No read of its own: the app reads what it shows.
    WorkspaceChanged {
        task_id: String,
    },
    /// The shells of a task changed, whether the app shows its rows or not.
    ShellsChanged {
        task_id: String,
    },
    /// The review comments of a task changed. No read of its own: the Changes tab reads them.
    ReviewCommentsChanged {
        task_id: String,
    },
    /// A change added a line to the task's log. No read of its own: the app reads the events,
    /// stage files and settings of the task it shows.
    TaskLogged {
        task_id: String,
    },
}

/// Why the feed stops following one socket.
pub(crate) enum Halt {
    AppGone,
    Refused,
    Lost(ClientError),
}

/// What needs a read. `dirty`: files changed in some workspace, so the diff stats are read at
/// most once per `diff_stats_every`; `diff_stats`: read them now.
#[derive(Default)]
pub(crate) struct Stale {
    tasks: bool,
    repos: bool,
    preparing: bool,
    jobs: bool,
    activity: bool,
    usage: bool,
    limits: bool,
    diff_stats: bool,
    dirty: bool,
    dirty_read_at: Option<Instant>,
    /// Task ids that go out as they are, with no read.
    passed: Vec<DashboardEvent>,
    pub terminals: BTreeSet<String>,
}

impl Stale {
    /// Everything, as after a connect: messages sent while no socket was open are lost.
    pub fn everything(shown: &BTreeSet<String>) -> Stale {
        Stale {
            tasks: true,
            repos: true,
            preparing: true,
            terminals: shown.clone(),
            ..Stale::polled()
        }
    }

    /// What the fallback poll reads: the parts the web polls every minute.
    pub fn poll(&mut self) {
        (self.jobs, self.activity, self.usage, self.diff_stats) = (true, true, true, true);
        self.limits = true;
    }

    fn polled() -> Stale {
        let mut stale = Stale::default();
        stale.poll();
        stale
    }

    /// Marks what one message makes out of date. A workspace step comes back to pass on at once.
    pub fn note(&mut self, text: &str, shown: &BTreeSet<String>) -> Option<PreparingMessage> {
        match EventKind::parse(text).and_then(EventKind::resource)? {
            Resource::Tasks => {
                self.tasks = true;
                if is_task_updated(text)
                    && let Some(task_id) = task_id_of(text)
                {
                    self.passed.push(DashboardEvent::TaskLogged { task_id });
                }
            }
            Resource::Repos => self.repos = true,
            Resource::Jobs => self.jobs = true,
            Resource::Activity => self.activity = true,
            Resource::Usage => self.usage = true,
            Resource::Limits => self.limits = true,
            Resource::DiffStats => {
                self.dirty = true;
                if let Some(task_id) = task_id_of(text) {
                    self.passed
                        .push(DashboardEvent::WorkspaceChanged { task_id });
                }
            }
            Resource::Shells => {
                if let Some(id) = task_id_of(text) {
                    self.passed.push(DashboardEvent::ShellsChanged {
                        task_id: id.clone(),
                    });
                    if shown.contains(&id) {
                        self.terminals.insert(id);
                    }
                }
            }
            Resource::Preparing => return PreparingMessage::parse(text),
            Resource::ReviewComments => {
                if let Some(task_id) = task_id_of(text) {
                    self.passed
                        .push(DashboardEvent::ReviewCommentsChanged { task_id });
                }
            }
            Resource::Resync => {
                let dirty_read_at = self.dirty_read_at;
                *self = Stale::everything(shown);
                self.dirty_read_at = dirty_read_at;
            }
        }
        None
    }

    /// When the diff stats that wait for their time are due.
    pub fn due(&self, every: Duration) -> Option<Instant> {
        if !self.dirty {
            return None;
        }
        self.dirty_read_at.map(|at| at + every)
    }

    /// One read of each part that is out of date, task list first.
    pub async fn read(
        &mut self,
        config: &Config,
        diff_stats_every: Duration,
        live: &mut bool,
        events: &UnboundedSender<DashboardEvent>,
    ) -> Result<(), Halt> {
        for event in std::mem::take(&mut self.passed) {
            send(events, event)?;
        }
        if std::mem::take(&mut self.tasks) {
            match tasks::api::list_tasks(config).await {
                Ok(list) => {
                    if !*live {
                        *live = true;
                        send(events, DashboardEvent::Status(LinkStatus::Live))?;
                    }
                    send(events, DashboardEvent::Tasks(list))?;
                }
                Err(ClientError::Unauthorized) => return Err(Halt::Refused),
                Err(error) => return Err(Halt::Lost(error)),
            }
        }
        if std::mem::take(&mut self.repos) {
            let read = projects::api::list_repos(config).await;
            pass(read, events, |list| DashboardEvent::Repos {
                repos: list.repos,
                worktrees_dir: list.worktrees_dir,
            })?;
        }
        if std::mem::take(&mut self.preparing) {
            let read = workspaces::api::list_preparing(config).await;
            pass(read, events, DashboardEvent::Preparing)?;
        }
        if std::mem::take(&mut self.jobs) {
            pass(
                tasks::api::list_jobs(config).await,
                events,
                DashboardEvent::Jobs,
            )?;
        }
        if std::mem::take(&mut self.activity) {
            let read = agents::api::activity(config).await;
            pass(read, events, DashboardEvent::Activity)?;
        }
        if std::mem::take(&mut self.usage) {
            pass(
                agents::api::usage(config).await,
                events,
                DashboardEvent::Usage,
            )?;
        }
        if std::mem::take(&mut self.limits) {
            pass(
                agents::api::limits(config).await,
                events,
                DashboardEvent::Limits,
            )?;
        }
        self.read_diff_stats(config, diff_stats_every, events)
            .await?;
        for task_id in std::mem::take(&mut self.terminals) {
            let read = terminals::api::list_terminals(config, &task_id).await;
            pass(read, events, |terminals| DashboardEvent::Terminals {
                task_id,
                terminals,
            })?;
        }
        Ok(())
    }

    /// The daemon runs git for each workspace that changed, and with busy agents one always has:
    /// so a change reads the stats at most once per `every` (`DIFFSTATS_EVERY_MS`).
    async fn read_diff_stats(
        &mut self,
        config: &Config,
        every: Duration,
        events: &UnboundedSender<DashboardEvent>,
    ) -> Result<(), Halt> {
        let now = Instant::now();
        let open = self.dirty_read_at.is_none_or(|at| now >= at + every);
        if !self.diff_stats && !(self.dirty && open) {
            return Ok(());
        }
        if self.dirty && open {
            self.dirty_read_at = Some(now);
        }
        (self.diff_stats, self.dirty) = (false, false);
        let read = workspaces::api::diff_stats(config).await;
        pass(read, events, DashboardEvent::DiffStats)
    }
}

pub(crate) fn send(
    events: &UnboundedSender<DashboardEvent>,
    event: DashboardEvent,
) -> Result<(), Halt> {
    events.unbounded_send(event).map_err(|_| Halt::AppGone)
}

/// A read other than the task list that fails on its own, such as the terminals of a task just
/// deleted, is left out: the rest of the data still comes.
fn pass<T>(
    read: Result<T, ClientError>,
    events: &UnboundedSender<DashboardEvent>,
    event: impl FnOnce(T) -> DashboardEvent,
) -> Result<(), Halt> {
    match read {
        Ok(value) => send(events, event(value)),
        Err(ClientError::Unauthorized) => Err(Halt::Refused),
        Err(error @ ClientError::Unreachable { .. }) => Err(Halt::Lost(error)),
        Err(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shown(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|id| id.to_string()).collect()
    }

    #[test]
    fn shells_of_a_task_not_shown_are_not_read() {
        let mut stale = Stale::default();
        let shown = shown(&["T-1"]);
        stale.note(r#"{"kind":"shells.changed","taskId":"T-2"}"#, &shown);
        assert!(stale.terminals.is_empty());
        stale.note(r#"{"kind":"shells.changed","taskId":"T-1"}"#, &shown);
        assert_eq!(stale.terminals, shown);
    }

    #[test]
    fn shells_and_workspace_changes_pass_every_task_id_on() {
        let mut stale = Stale::default();
        stale.note(
            r#"{"kind":"shells.changed","taskId":"T-2"}"#,
            &shown(&["T-1"]),
        );
        stale.note(r#"{"kind":"workspace.dirty","taskId":"T-3"}"#, &shown(&[]));
        stale.note(
            r#"{"kind":"review-comments.changed","taskId":"T-4"}"#,
            &shown(&[]),
        );
        assert_eq!(
            stale.passed,
            [
                DashboardEvent::ShellsChanged {
                    task_id: "T-2".into()
                },
                DashboardEvent::WorkspaceChanged {
                    task_id: "T-3".into()
                },
                DashboardEvent::ReviewCommentsChanged {
                    task_id: "T-4".into()
                },
            ]
        );
    }

    #[test]
    fn a_workspace_step_is_passed_on_and_marks_nothing() {
        let mut stale = Stale::default();
        let step = stale.note(
            r#"{"kind":"workspace.preparing","taskId":"T-1","step":"fetch"}"#,
            &BTreeSet::new(),
        );
        assert_eq!(step.map(|step| step.task_id).as_deref(), Some("T-1"));
        assert!(!stale.preparing && !stale.tasks);
    }

    #[test]
    fn resync_marks_everything_but_keeps_the_diff_stats_time() {
        let at = Instant::now();
        let mut stale = Stale {
            dirty_read_at: Some(at),
            ..Stale::default()
        };
        stale.note(r#"{"kind":"resync"}"#, &shown(&["T-1"]));
        assert!(stale.tasks && stale.repos && stale.preparing && stale.diff_stats);
        assert!(stale.jobs && stale.activity && stale.usage);
        assert_eq!(stale.terminals, shown(&["T-1"]));
        assert_eq!(stale.dirty_read_at, Some(at));
    }

    #[test]
    fn diff_stats_wait_only_after_a_read_for_a_change() {
        let every = Duration::from_secs(15);
        let mut stale = Stale::default();
        stale.note(
            r#"{"kind":"workspace.dirty","taskId":"T-1"}"#,
            &BTreeSet::new(),
        );
        assert_eq!(stale.due(every), None);
        let at = Instant::now();
        stale.dirty_read_at = Some(at);
        assert_eq!(stale.due(every), Some(at + every));
    }
}
