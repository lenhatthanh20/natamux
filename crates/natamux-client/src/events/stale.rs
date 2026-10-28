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
use crate::events::task_body::{has_moved, task_of};
use crate::terminals::backoff::reconnect_delay;
use crate::{agents, projects, tasks, terminals, workspaces};

/// Task list reads in a row that fail before the daemon is taken for gone.
const TASK_LIST_TRIES: u32 = 3;

/// The dashboard data, whole each time one part of it is read again.
#[derive(Clone, Debug, PartialEq)]
pub enum DashboardEvent {
    Tasks(Vec<Task>),
    /// One task, from the body of its message: no read of the list.
    TaskChanged(Box<Task>),
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
    /// Events that go out as they are, with no read: task ids, and tasks from message bodies.
    passed: Vec<DashboardEvent>,
    pub terminals: BTreeSet<String>,
    /// Parts whose last read failed to connect or timed out: they stay stale and wait here.
    /// The key is the part's name, or `terminals:<task id>`.
    retries: BTreeMap<String, Retry>,
    /// Task list reads that failed in a row.
    task_failures: u32,
}

/// The wait before a part is read again after a read that did not get an answer.
#[derive(Clone, Copy)]
struct Retry {
    tries: u32,
    not_before: Instant,
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

    /// What the fallback poll reads: the parts the web polls every minute, and the task list,
    /// which the task bodies keep up to date otherwise (a task file edited by hand shows here).
    pub fn poll(&mut self) {
        (self.jobs, self.activity, self.usage, self.diff_stats) = (true, true, true, true);
        (self.limits, self.tasks) = (true, true);
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
                // A move to another step is rare, so the whole list is read then too: a check
                // that the merged list still matches. `task.deleted` has no task.
                match task_of(text).filter(|_| !has_moved(text)).map(Box::new) {
                    Some(task) => self.passed.push(DashboardEvent::TaskChanged(task)),
                    None => self.tasks = true,
                }
                // After the task: the app reads the log of the task it holds now.
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
                let retries = std::mem::take(&mut self.retries);
                let task_failures = self.task_failures;
                *self = Stale::everything(shown);
                self.dirty_read_at = dirty_read_at;
                // Kept: a daemon that is not answering must not be asked at once again.
                (self.retries, self.task_failures) = (retries, task_failures);
            }
        }
        None
    }

    /// When the next read that waits for its time is due: the diff stats, or a part read again
    /// after a failure.
    pub fn due(&self, every: Duration) -> Option<Instant> {
        // Left out while its retry waits: the retry time is the next wake-up, and the old read
        // time is already past, so it would wake the feed in a busy loop.
        let diff_stats = self
            .dirty
            .then_some(self.dirty_read_at)
            .flatten()
            .filter(|_| self.ready("diff_stats"))
            .map(|at| at + every);
        let retries = self.retries.values().map(|retry| retry.not_before);
        diff_stats.into_iter().chain(retries).min()
    }

    /// False while the part waits after a failed read.
    fn ready(&self, key: &str) -> bool {
        self.retries
            .get(key)
            .is_none_or(|retry| Instant::now() >= retry.not_before)
    }

    fn back_off(&mut self, key: &str) {
        let tries = self.retries.get(key).map_or(0, |retry| retry.tries);
        let wait = reconnect_delay(tries, fastrand::f64());
        let retry = Retry {
            tries: tries.saturating_add(1),
            not_before: Instant::now() + wait,
        };
        self.retries.insert(key.to_string(), retry);
    }

    /// Sends what a read gave. True: the read got no answer, so the part stays stale. A read that
    /// fails on its own, such as the terminals of a task just deleted, is left out: the rest of
    /// the data still comes.
    fn pass<T>(
        &mut self,
        key: &str,
        read: Result<T, ClientError>,
        events: &UnboundedSender<DashboardEvent>,
        event: impl FnOnce(T) -> DashboardEvent,
    ) -> Result<bool, Halt> {
        let retry = match read {
            Ok(value) => {
                send(events, event(value))?;
                false
            }
            Err(ClientError::Unauthorized) => return Err(Halt::Refused),
            Err(ClientError::Unreachable { .. }) => true,
            Err(_) => false,
        };
        if retry {
            self.back_off(key);
        } else {
            self.retries.remove(key);
        }
        Ok(retry)
    }

    /// One read of each part that is out of date, task list first. A read that gets no answer
    /// leaves its part stale for a later read: only the socket decides that the link is lost,
    /// with the task list as the one exception. The passed events go out before the reads: a
    /// list read now is newer than every task body noted before it, so it must come last.
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
        if self.tasks && self.ready("tasks") {
            self.tasks = false;
            self.read_tasks(config, live, events).await?;
        }
        if self.repos && self.ready("repos") {
            let read = projects::api::list_repos(config).await;
            self.repos = self.pass("repos", read, events, |list| DashboardEvent::Repos {
                repos: list.repos,
                worktrees_dir: list.worktrees_dir,
            })?;
        }
        if self.preparing && self.ready("preparing") {
            let read = workspaces::api::list_preparing(config).await;
            self.preparing = self.pass("preparing", read, events, DashboardEvent::Preparing)?;
        }
        if self.jobs && self.ready("jobs") {
            let read = tasks::api::list_jobs(config).await;
            self.jobs = self.pass("jobs", read, events, DashboardEvent::Jobs)?;
        }
        if self.activity && self.ready("activity") {
            let read = agents::api::activity(config).await;
            self.activity = self.pass("activity", read, events, DashboardEvent::Activity)?;
        }
        if self.usage && self.ready("usage") {
            let read = agents::api::usage(config).await;
            self.usage = self.pass("usage", read, events, DashboardEvent::Usage)?;
        }
        if self.limits && self.ready("limits") {
            let read = agents::api::limits(config).await;
            self.limits = self.pass("limits", read, events, DashboardEvent::Limits)?;
        }
        self.read_diff_stats(config, diff_stats_every, events)
            .await?;
        for task_id in std::mem::take(&mut self.terminals) {
            let key = format!("terminals:{task_id}");
            if !self.ready(&key) {
                self.terminals.insert(task_id);
                continue;
            }
            let read = terminals::api::list_terminals(config, &task_id).await;
            let id = task_id.clone();
            let again = self.pass(&key, read, events, |terminals| DashboardEvent::Terminals {
                task_id: id,
                terminals,
            })?;
            if again {
                self.terminals.insert(task_id);
            }
        }
        // A task that is no longer shown must not keep the feed waking up for it.
        let shown = &self.terminals;
        self.retries.retain(|key, _| {
            key.strip_prefix("terminals:")
                .is_none_or(|id| shown.contains(id))
        });
        Ok(())
    }

    /// The daemon is likely gone after `TASK_LIST_TRIES` failed reads in a row: the socket alone
    /// can take long to notice it.
    async fn read_tasks(
        &mut self,
        config: &Config,
        live: &mut bool,
        events: &UnboundedSender<DashboardEvent>,
    ) -> Result<(), Halt> {
        match tasks::api::list_tasks(config).await {
            Ok(list) => {
                self.task_failures = 0;
                self.retries.remove("tasks");
                if !*live {
                    *live = true;
                    send(events, DashboardEvent::Status(LinkStatus::Live))?;
                }
                send(events, DashboardEvent::Tasks(list))
            }
            Err(ClientError::Unauthorized) => Err(Halt::Refused),
            Err(ClientError::Unreachable { url, reason }) => {
                self.task_failures += 1;
                if self.task_failures >= TASK_LIST_TRIES {
                    let reason = format!(
                        "the task list could not be read {TASK_LIST_TRIES} times: {reason}"
                    );
                    return Err(Halt::Lost(ClientError::Unreachable { url, reason }));
                }
                self.tasks = true;
                self.back_off("tasks");
                Ok(())
            }
            // An answer that is not 2xx: the daemon is there, but the list is not.
            Err(error) => Err(Halt::Lost(error)),
        }
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
        if (!self.diff_stats && !(self.dirty && open)) || !self.ready("diff_stats") {
            return Ok(());
        }
        if self.dirty && open {
            self.dirty_read_at = Some(now);
        }
        (self.diff_stats, self.dirty) = (false, false);
        let read = workspaces::api::diff_stats(config).await;
        self.diff_stats = self.pass("diff_stats", read, events, DashboardEvent::DiffStats)?;
        Ok(())
    }
}

pub(crate) fn send(
    events: &UnboundedSender<DashboardEvent>,
    event: DashboardEvent,
) -> Result<(), Halt> {
    events.unbounded_send(event).map_err(|_| Halt::AppGone)
}

#[cfg(test)]
#[path = "stale_tests.rs"]
mod tests;
