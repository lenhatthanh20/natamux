//! Where the app's data comes from: the daemon, or the data of `--fake-daemon` for CI
//! screenshots. Each call is the same route of the daemon either way.

use std::collections::BTreeSet;
use std::sync::{Arc, Mutex, PoisonError};

use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender};
use futures::executor::block_on;
use natamux_client::{
    Answer, Client, ClientError, DashboardEvent, SendEventAnswer, SessionOptions, TerminalHandle,
    TerminalRows,
};
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::events::task_event::{StoredEvent, TaskEventInput};
use natamux_proto::projects::repo::{BranchList, LoadedSettings, SkillList};
use natamux_proto::tasks::task::{DeleteTaskAnswer, NewTaskInput, Task, TemplateTaskInput};
use natamux_proto::tasks::template::TaskTemplate;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{
    ChangeEntry, GitGraph, NewWorkspaceInput, RemoveWorkspaceAnswer,
};

use super::fake_daemon::{self, FakeDaemon};
use crate::tasks::delete_finished::{DeleteOutcome, delete_each};
#[cfg(test)]
use crate::workspaces::remove_finished_rules::remove_summary;
use crate::workspaces::remove_finished_rules::{RemoveOutcome, remove_each};

pub enum Source {
    Daemon(Arc<Client>),
    FakeDaemon,
}

/// The dashboard data now and after every change, and where the app says which tasks show
/// terminal rows.
pub struct Feed {
    pub events: UnboundedReceiver<DashboardEvent>,
    pub rows: Rows,
}

pub enum Rows {
    Daemon(TerminalRows),
    Fake {
        feed: UnboundedSender<DashboardEvent>,
        shown: Mutex<BTreeSet<String>>,
    },
}

impl Rows {
    /// The tasks whose terminal rows the app shows. Newly shown tasks are read at once.
    pub fn show(&self, task_ids: impl IntoIterator<Item = String>) {
        match self {
            Rows::Daemon(rows) => rows.show(task_ids),
            Rows::Fake { feed, shown } => {
                let ids: BTreeSet<String> = task_ids.into_iter().collect();
                let mut shown = shown.lock().unwrap_or_else(PoisonError::into_inner);
                let added = ids.difference(&shown).cloned().collect();
                *shown = ids;
                fake_daemon::shared().send_terminals(feed, &added);
            }
        }
    }
}

impl Source {
    pub fn watch_dashboard(&self) -> Feed {
        match self {
            Source::Daemon(client) => {
                let feed = client.watch_dashboard();
                Feed {
                    events: feed.events,
                    rows: Rows::Daemon(feed.terminals),
                }
            }
            Source::FakeDaemon => {
                let (events, feed) = fake_daemon::shared().watch();
                Feed {
                    events,
                    rows: Rows::Fake {
                        feed,
                        shown: Mutex::new(BTreeSet::new()),
                    },
                }
            }
        }
    }

    pub fn open_terminal(&self, options: SessionOptions) -> TerminalHandle {
        match self {
            Source::Daemon(client) => client.open_terminal(options),
            Source::FakeDaemon => TerminalHandle::detached(fake_daemon::terminal()),
        }
    }

    /// The daemon's request, or the fake's answer, ready at once.
    fn ask<T: Send + 'static>(
        &self,
        daemon: impl FnOnce(&Client) -> Answer<T>,
        fake: impl FnOnce(&FakeDaemon) -> Result<T, ClientError>,
    ) -> Answer<T> {
        match self {
            Source::Daemon(client) => daemon(client),
            Source::FakeDaemon => ready(fake(fake_daemon::shared())),
        }
    }

    pub fn list_templates(&self) -> Answer<Vec<TaskTemplate>> {
        self.ask(|c| c.list_templates(), |f| f.list_templates())
    }

    pub fn list_events(&self, task_id: &str) -> Answer<Vec<StoredEvent>> {
        self.ask(|c| c.list_events(task_id), |f| f.list_events(task_id))
    }

    pub fn list_branches(&self, repo: Option<&str>) -> Answer<BranchList> {
        self.ask(|c| c.list_branches(repo), |f| f.list_branches(repo))
    }

    pub fn load_settings(&self, repo_id: &str) -> Answer<LoadedSettings> {
        self.ask(|c| c.load_settings(repo_id), |f| f.load_settings(repo_id))
    }

    pub fn list_skills(&self, repo_id: &str) -> Answer<SkillList> {
        self.ask(|c| c.list_skills(repo_id), |f| f.list_skills(repo_id))
    }

    /// The files not committed in a task's workspace.
    pub fn list_changes(&self, task_id: &str) -> Answer<Vec<ChangeEntry>> {
        self.ask(|c| c.list_changes(task_id), |f| f.list_changes(task_id))
    }

    pub fn graph(&self, task_id: &str) -> Answer<GitGraph> {
        self.ask(|c| c.graph(task_id), |f| f.graph(task_id))
    }

    pub fn list_terminals(&self, task_id: &str) -> Answer<TerminalList> {
        self.ask(|c| c.list_terminals(task_id), |f| f.list_terminals(task_id))
    }

    pub fn create_task(&self, input: NewTaskInput) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.create_task(input),
            Source::FakeDaemon => ready(fake_daemon::shared().create_task(input)),
        }
    }

    pub fn create_from_template(
        &self,
        template_id: &str,
        input: TemplateTaskInput,
    ) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.create_from_template(template_id, input),
            Source::FakeDaemon => {
                ready(fake_daemon::shared().create_from_template(template_id, input))
            }
        }
    }

    pub fn send_event(&self, task_id: &str, input: TaskEventInput) -> Answer<SendEventAnswer> {
        match self {
            Source::Daemon(client) => client.send_event(task_id, input),
            Source::FakeDaemon => ready(fake_daemon::shared().send_event(task_id, input)),
        }
    }

    pub fn rename_task(&self, task_id: &str, title: &str) -> Answer<Task> {
        self.ask(
            |c| c.rename_task(task_id, title),
            |f| f.rename_task(task_id, title),
        )
    }

    pub fn delete_task(&self, task_id: &str) -> Answer<DeleteTaskAnswer> {
        self.ask(|c| c.delete_task(task_id), |f| f.delete_task(task_id))
    }

    pub fn remove_repo(&self, repo_id: &str) -> Answer<()> {
        self.ask(|c| c.remove_repo(repo_id), |f| f.remove_repo(repo_id))
    }

    pub fn new_workspace(&self, input: NewWorkspaceInput) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.new_workspace(input),
            Source::FakeDaemon => ready(fake_daemon::shared().new_workspace(input)),
        }
    }

    pub fn remove_workspace(
        &self,
        task_id: &str,
        delete_branch: bool,
    ) -> Answer<RemoveWorkspaceAnswer> {
        self.ask(
            |c| c.remove_workspace(task_id, delete_branch),
            |f| f.remove_workspace(task_id, delete_branch),
        )
    }

    pub fn start_agent(&self, task_id: &str, input: StartAgentInput) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.start_agent(task_id, input),
            Source::FakeDaemon => ready(fake_daemon::shared().start_agent(task_id, input)),
        }
    }

    /// Ends a terminal of a task and its shell.
    pub fn close_terminal(&self, task_id: &str, terminal: &str) -> Answer<()> {
        self.ask(
            |c| c.close_terminal(task_id, terminal),
            |f| f.close_terminal(task_id, terminal),
        )
    }
}

fn ready<T: Send + 'static>(answer: Result<T, ClientError>) -> Answer<T> {
    Box::pin(async move { answer })
}

// The bulk writes of the board and the finished fold. They block, so they run off the main
// thread, and each call waits for the one before: git work on one repository must not race.

/// Delete all done or cancelled: the outcomes, and the tasks the daemon changed with them.
pub fn delete_all(source: &Source, ids: &[String]) -> (Vec<DeleteOutcome>, Vec<Task>) {
    let mut related = Vec::new();
    let outcomes = delete_each(
        ids,
        |id| block_on(source.delete_task(id)).map_err(|error| reason(&error)),
        |answer| related.extend(answer.tasks),
    );
    (outcomes, related)
}

/// Remove finished workspaces. A merged branch goes too, as the web's `removeWorkspace(id)`.
pub fn remove_all(source: &Source, ids: &[String]) -> (Vec<RemoveOutcome>, Vec<Task>) {
    let mut changed = Vec::new();
    let outcomes = remove_each(
        ids,
        |id| block_on(source.remove_workspace(id, true)).map_err(|error| reason(&error)),
        |answer| changed.push(answer.task.clone()),
    );
    (outcomes, changed)
}

/// The daemon's own message, which starts with the task id, as the web's toasts show it: not
/// "The daemon answered 409: …".
pub fn reason(error: &ClientError) -> String {
    match error {
        ClientError::Status { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_finished_workspaces_deletes_merged_branches() {
        let (outcomes, changed) = remove_all(&Source::FakeDaemon, &["T-0120".to_string()]);
        assert_eq!(
            outcomes,
            [RemoveOutcome::Removed {
                id: "T-0120".into(),
                branch_deleted: true,
            }]
        );
        assert!(changed[0].branch.is_none());
        assert_eq!(remove_summary(&outcomes).text, "Removed 1 workspace.");
    }

    #[test]
    fn a_failure_reason_is_the_daemons_own_message() {
        let refused = ClientError::Status {
            status: 409,
            message: "T-5 has subtasks".into(),
        };
        assert_eq!(reason(&refused), "T-5 has subtasks");
        assert_eq!(
            reason(&ClientError::Unauthorized),
            ClientError::Unauthorized.to_string()
        );
    }
}
