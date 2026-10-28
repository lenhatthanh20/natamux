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
use natamux_proto::projects::repo::{BranchList, LoadedSettings, RemoteBranchList, SkillList};
use natamux_proto::stages::actions::{StageActionInput, StageFile};
use natamux_proto::tasks::task::{
    DeleteTaskAnswer, NewTaskInput, Task, TaskChanges, TemplateTaskInput,
};
use natamux_proto::tasks::template::{TaskTemplate, TemplateInput};
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{
    ChangeEntry, FanOutInput, GitGraph, NewWorkspaceInput, RemoveWorkspaceAnswer,
};

use super::fake_daemon::{self, FakeDaemon};
use crate::tasks::delete_finished::{DeleteOutcome, delete_each};
use crate::tasks::details_commands::StageToggle;
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
            Source::FakeDaemon => TerminalHandle::detached(
                fake_daemon::shared().terminal(&options.task_id, &options.terminal),
            ),
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

    pub fn list_remote_branches(&self, repo: &str) -> Answer<RemoteBranchList> {
        self.ask(
            |c| c.list_remote_branches(repo),
            |f| f.list_remote_branches(repo),
        )
    }

    pub fn load_settings(&self, repo_id: &str) -> Answer<LoadedSettings> {
        self.ask(|c| c.load_settings(repo_id), |f| f.load_settings(repo_id))
    }

    pub fn list_skills(&self, repo_id: &str) -> Answer<SkillList> {
        self.ask(|c| c.list_skills(repo_id), |f| f.list_skills(repo_id))
    }

    /// The files not committed in a task's workspace.
    pub fn list_changes(&self, task_id: &str) -> Answer<Vec<ChangeEntry>> {
        self.ask(
            |c| c.list_changes(task_id, false),
            |f| f.list_changes(task_id, false),
        )
    }

    pub fn graph(&self, task_id: &str) -> Answer<GitGraph> {
        self.ask(|c| c.graph(task_id, false), |f| f.graph(task_id, false))
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

    /// `force` discards changes that are not committed; the fake daemon has none.
    pub fn delete_task(&self, task_id: &str, force: bool) -> Answer<DeleteTaskAnswer> {
        self.ask(
            |c| c.delete_task(task_id, force),
            |f| f.delete_task(task_id),
        )
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
        force: bool,
    ) -> Answer<RemoveWorkspaceAnswer> {
        self.ask(
            |c| c.remove_workspace(task_id, delete_branch, force),
            |f| f.remove_workspace(task_id, delete_branch),
        )
    }

    pub fn start_agent(&self, task_id: &str, input: StartAgentInput) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.start_agent(task_id, input),
            Source::FakeDaemon => ready(fake_daemon::shared().start_agent(task_id, input)),
        }
    }

    pub fn create_workspace(&self, task_id: &str, base: Option<&str>) -> Answer<Task> {
        self.ask(
            |c| c.create_workspace(task_id, base),
            |f| f.create_workspace(task_id, base),
        )
    }

    pub fn push(&self, task_id: &str) -> Answer<Task> {
        self.ask(|c| c.push(task_id), |f| f.push(task_id))
    }

    pub fn open_pull_request(&self, task_id: &str) -> Answer<Task> {
        self.ask(
            |c| c.open_pull_request(task_id),
            |f| f.open_pull_request(task_id),
        )
    }

    pub fn reopen_agent(&self, task_id: &str) -> Answer<Task> {
        self.ask(|c| c.reopen_agent(task_id), |f| f.reopen_agent(task_id))
    }

    /// The last screens of the terminals a finished task ended.
    pub fn ended_shells(&self, task_id: &str) -> Answer<Vec<EndedShell>> {
        self.ask(|c| c.ended_shells(task_id), |f| f.ended_shells(task_id))
    }

    /// Ends a terminal of a task and its shell.
    pub fn close_terminal(&self, task_id: &str, terminal: &str) -> Answer<()> {
        self.ask(
            |c| c.close_terminal(task_id, terminal),
            |f| f.close_terminal(task_id, terminal),
        )
    }

    // Screen 3: the details panel.

    pub fn stage_action(&self, task_id: &str, input: StageActionInput) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.stage_action(task_id, input),
            Source::FakeDaemon => ready(fake_daemon::shared().stage_action(task_id, input)),
        }
    }

    pub fn set_stage_toggle(
        &self,
        task_id: &str,
        kind: StageToggle,
        stage: &str,
        on: bool,
    ) -> Answer<Task> {
        match (self, kind) {
            (Source::Daemon(client), StageToggle::Workflow) => {
                client.set_workflow(task_id, stage, on)
            }
            (Source::Daemon(client), StageToggle::Ci) => client.set_ci(task_id, stage, on),
            (Source::Daemon(client), StageToggle::Skip) => client.set_skip(task_id, stage, on),
            (Source::FakeDaemon, kind) => {
                ready(fake_daemon::shared().set_stage_toggle(task_id, kind.name(), stage, on))
            }
        }
    }

    pub fn list_stage_files(&self, task_id: &str) -> Answer<Vec<StageFile>> {
        self.ask(
            |c| c.list_stage_files(task_id),
            |f| f.list_stage_files(task_id),
        )
    }

    pub fn update_task(&self, task_id: &str, changes: TaskChanges) -> Answer<Task> {
        match self {
            Source::Daemon(client) => client.update_task(task_id, changes),
            Source::FakeDaemon => ready(fake_daemon::shared().update_task(task_id, changes)),
        }
    }

    pub fn save_template(&self, input: TemplateInput) -> Answer<TaskTemplate> {
        match self {
            Source::Daemon(client) => client.save_template(input),
            Source::FakeDaemon => ready(fake_daemon::shared().save_template(input)),
        }
    }

    pub fn fan_out(&self, task_id: &str, input: FanOutInput) -> Answer<Vec<Task>> {
        match self {
            Source::Daemon(client) => client.fan_out(task_id, input),
            Source::FakeDaemon => ready(fake_daemon::shared().fan_out(task_id, input)),
        }
    }

    pub fn create_issue(&self, task_id: &str) -> Answer<Task> {
        self.ask(|c| c.create_issue(task_id), |f| f.create_issue(task_id))
    }

    pub fn refresh_pull_request(&self, task_id: &str) -> Answer<Task> {
        self.ask(
            |c| c.refresh_pull_request(task_id),
            |f| f.refresh_pull_request(task_id),
        )
    }
}

fn ready<T: Send + 'static>(answer: Result<T, ClientError>) -> Answer<T> {
    Box::pin(async move { answer })
}

// The bulk writes of the board and the finished fold. They block, so they run off the main
// thread, and each call waits for the one before: git work on one repository must not race.

/// Delete all done or cancelled: the outcomes, and the tasks the daemon changed with them.
/// `force`: the tasks the confirm named for files that are not committed; those files are lost.
pub fn delete_all(
    source: &Source,
    ids: &[String],
    force: &[String],
) -> (Vec<DeleteOutcome>, Vec<Task>) {
    let mut related = Vec::new();
    let outcomes = delete_each(
        ids,
        |id| block_on(source.delete_task(id, forced(force, id))).map_err(|error| reason(&error)),
        |answer| related.extend(answer.tasks),
    );
    (outcomes, related)
}

/// Remove finished workspaces. A merged branch goes too, as the web's `removeWorkspace(id)`.
/// `force` as in `delete_all`.
pub fn remove_all(
    source: &Source,
    ids: &[String],
    force: &[String],
) -> (Vec<RemoveOutcome>, Vec<Task>) {
    let mut changed = Vec::new();
    let outcomes = remove_each(
        ids,
        |id| {
            block_on(source.remove_workspace(id, true, forced(force, id)))
                .map_err(|error| reason(&error))
        },
        |answer| changed.push(answer.task.clone()),
    );
    (outcomes, changed)
}

/// Whether `id` is one of the `force` ids of `delete_all` and `remove_all`.
fn forced(force: &[String], id: &str) -> bool {
    force.iter().any(|f| f == id)
}

/// The tasks among `ids` with files that are not committed, as (task id, file count). The reads
/// run at once. A failed read counts as none: the daemon then still refuses that one.
pub fn read_uncommitted(
    source: &Source,
    ids: &[String],
) -> impl Future<Output = Vec<(String, usize)>> + use<> {
    let reads: Vec<_> = ids
        .iter()
        .map(|id| {
            let id = id.clone();
            let changes = source.list_changes(&id);
            async move { (id, changes.await.map_or(0, |files| files.len())) }
        })
        .collect();
    async move {
        let counts = futures::future::join_all(reads).await;
        counts.into_iter().filter(|(_, count)| *count > 0).collect()
    }
}

/// The daemon's own message, which starts with the task id, as the web's toasts show it: not
/// "The daemon answered 409: …".
pub fn reason(error: &ClientError) -> String {
    match error {
        ClientError::Status { message, .. } => message.clone(),
        other => other.to_string(),
    }
}

#[path = "source_git.rs"]
mod git;

#[path = "source_settings.rs"]
mod settings;

#[path = "source_projects.rs"]
mod projects;

#[path = "source_files.rs"]
mod files;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn removing_finished_workspaces_deletes_merged_branches() {
        let (outcomes, changed) = remove_all(&Source::FakeDaemon, &["T-0120".to_string()], &[]);
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
    fn forces_only_the_ids_the_confirm_named() {
        let ids = ["T-A", "T-B", "T-C"];
        let force = ["T-B".to_string()];
        let mapped: Vec<bool> = ids.iter().map(|id| forced(&force, id)).collect();
        assert_eq!(mapped, [false, true, false]);
        assert!(ids.iter().all(|id| !forced(&[], id)));
    }

    #[test]
    fn reads_which_tasks_have_files_that_are_not_committed() {
        let ids = ["T-0143", "T-0146", "T-0999"].map(String::from);
        // T-0146 and T-0999, which does not exist, have none.
        assert_eq!(
            block_on(read_uncommitted(&Source::FakeDaemon, &ids)),
            [("T-0143".to_string(), 2)]
        );
        assert!(block_on(read_uncommitted(&Source::FakeDaemon, &[])).is_empty());
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
