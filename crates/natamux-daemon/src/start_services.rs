//! The workspace service and the GitHub services of the daemon, wired to the terminals and agents.

use std::sync::Arc;

use natamux_git::gh::default_run_gh;
use natamux_proto::tasks::task::Task;
use natamux_store::task_index::TaskIndex;

use super::TaskStore;
use crate::agents::generic_signals::GenericSignals;
use crate::agents::runner::AgentRunner;
use crate::config::Config;
use crate::github::issue_importer::{IssueImporter, IssueImporterOptions};
use crate::github::issue_linker::{IssueLinker, IssueLinkerOptions, LinkError};
use crate::github::pull_request_watcher::{PullRequestWatcher, PullRequestWatcherOptions};
use crate::projects::repos::{RepoOptions, Repos, TaskRef};
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::{Jobs, system_clock};
use crate::shared::scheduler::Scheduler;
use crate::tasks::errors::TaskError;
use crate::tasks::service::{TaskService, TaskServiceOptions};
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

fn io_error(error: TaskError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// The task store of the data folder, loaded, and what shares it.
pub(super) fn task_store(config: &Config) -> std::io::Result<TaskStore> {
    let index =
        TaskIndex::open(&config.data_dir.join("index.sqlite")).map_err(std::io::Error::other)?;
    let bus = EventBus::new();
    let jobs = Arc::new(Jobs::new(Some(Arc::clone(&bus)), system_clock()));
    // A folder that is gone does not stop the start: only repos.json that cannot be read does.
    let repos = Repos::with_options(RepoOptions {
        data_dir: config.data_dir.clone(),
        default_path: config.repo_dir.clone(),
        worktrees_dir: Some(config.worktrees_dir.clone()),
        tasks: None,
        now: system_clock(),
    })
    .map_err(std::io::Error::other)?;
    let settings = Arc::clone(&repos);
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: config.data_dir.clone(),
        index,
        bus: Arc::clone(&bus),
        now: None,
        settings_of: Some(Arc::new(move |repo| settings.stages_of(repo))),
        clock: None,
    });
    tasks.init().map_err(io_error)?;
    // The task service is made after the projects, which count its workspaces.
    let listing = tasks.clone();
    repos.set_tasks(Arc::new(move || {
        listing
            .list_tasks(None)
            .unwrap_or_default()
            .into_iter()
            .map(|task| TaskRef {
                repo: task.repo,
                workspace: task.workspace,
            })
            .collect()
    }));
    let signals = Arc::new(GenericSignals::new(tasks.clone(), None));
    Ok(TaskStore {
        signals,
        tasks,
        repos,
        jobs,
        bus,
    })
}
