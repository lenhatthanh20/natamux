//! The workspace service and the GitHub services of the daemon, wired to the terminals and agents.

use std::sync::Arc;

use natamux_git::gh::default_run_gh;
use natamux_proto::tasks::task::Task;

use super::TaskStore;
use crate::agents::runner::AgentRunner;
use crate::config::Config;
use crate::github::issue_importer::{IssueImporter, IssueImporterOptions};
use crate::github::issue_linker::{IssueLinker, IssueLinkerOptions, LinkError};
use crate::github::pull_request_watcher::{PullRequestWatcher, PullRequestWatcherOptions};
use crate::shared::scheduler::Scheduler;
use crate::terminals::limits::MAIN_TERMINAL;
use crate::terminals::service::TerminalService;
use crate::terminals::types::DEFAULT_SIZE;
use crate::workspaces::agent_once::default_run_agent_once;
use crate::workspaces::errors::WorkspaceError;
use crate::workspaces::service::{WorkspaceOptions, WorkspaceService};
use crate::workspaces::setup_script::setup_command;

/// The issue importer, the issue linker (listening) and the pull request watcher (running).
pub(super) fn github_services(
    store: &TaskStore,
    config: &Config,
    scheduler: &Arc<Scheduler>,
) -> (
    Arc<IssueImporter>,
    Arc<IssueLinker>,
    Arc<PullRequestWatcher>,
) {
    let (importer_paths, linker_paths, watcher_paths) = (
        Arc::clone(&store.repos),
        Arc::clone(&store.repos),
        Arc::clone(&store.repos),
    );
    let settings = Arc::clone(&store.repos);
    let issues = Arc::new(IssueImporter::new(IssueImporterOptions {
        tasks: store.tasks.clone(),
        repo_path: Arc::new(move |repo| importer_paths.path_of(repo)),
        gh: default_run_gh(),
        jobs: Arc::clone(&store.jobs),
    }));
    let linker = Arc::new(IssueLinker::new(IssueLinkerOptions {
        tasks: store.tasks.clone(),
        bus: Arc::clone(&store.bus),
        data_dir: config.data_dir.clone(),
        repo_path: Arc::new(move |repo| linker_paths.path_of(repo)),
        settings_of: Arc::new(move |repo| settings.settings_of(repo)),
        gh: default_run_gh(),
        jobs: Arc::clone(&store.jobs),
    }));
    linker.start();
    let data_dir = config.data_dir.to_string_lossy().into_owned();
    let mut watching = PullRequestWatcherOptions::new(
        store.tasks.clone(),
        // A task of a removed project still has its pull request: gh runs in the data folder.
        Arc::new(move |repo| {
            watcher_paths
                .path_of(repo)
                .unwrap_or_else(|_| data_dir.clone())
        }),
        default_run_gh(),
        Arc::clone(scheduler),
    );
    watching.bus = Some(Arc::clone(&store.bus));
    let pull_requests = Arc::new(PullRequestWatcher::new(watching));
    // The Rust daemon has no login-PATH gate before phase 5, so it starts at once.
    pull_requests.start();
    (issues, linker, pull_requests)
}

/// The workspace service, which ends and types into the terminals and asks the agents for their
/// programs.
pub(super) fn workspace_service(
    config: &Config,
    store: &TaskStore,
    scheduler: &Arc<Scheduler>,
    terminals: &TerminalService,
    agents: &Arc<AgentRunner>,
    linker: &Arc<IssueLinker>,
) -> Arc<WorkspaceService> {
    let mut options = WorkspaceOptions::new(
        config.data_dir.clone(),
        config.worktrees_dir.clone(),
        store.tasks.clone(),
        Arc::clone(&store.repos),
        Arc::clone(&store.bus),
        Arc::clone(&store.jobs),
        Arc::clone(scheduler),
    );
    let closing = terminals.clone();
    options.close_terminals = Arc::new(move |task_id: &str| closing.close_and_wait(task_id).wait());
    let typing = terminals.clone();
    options.on_created = Some(Arc::new(move |task: &Task| {
        let command = task
            .workspace
            .as_deref()
            .and_then(|workspace| setup_command(std::path::Path::new(workspace), cfg!(windows)));
        if let Some(command) = command {
            // A failed setup run is not a failed workspace.
            let _ = typing.run(&task.id, &command, MAIN_TERMINAL, DEFAULT_SIZE);
        }
    }));
    let programs = Arc::clone(agents);
    options.program_of = Arc::new(move |agent: &str| programs.program_of(agent));
    let linking = Arc::clone(linker);
    options.pull_request_body =
        Arc::new(move |task: &Task| linking.pull_request_body(task).map_err(link_failure));
    options.run_agent_once = default_run_agent_once();
    WorkspaceService::new(options)
}

/// An issue that could not be opened for a pull request: 409 as Node, or the task's own error.
fn link_failure(error: LinkError) -> WorkspaceError {
    match error {
        LinkError::Task(error) => WorkspaceError::Task(error),
        LinkError::GitHub(error) => WorkspaceError::GitHub(error.message),
        LinkError::Link(error) => WorkspaceError::Conflict(error.to_string()),
    }
}
