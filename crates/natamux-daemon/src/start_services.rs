//! The workspace service, the GitHub services and the idle pause of the daemon, wired to the
//! terminals and agents.

use std::path::Path;
use std::sync::Arc;

use natamux_git::gh::default_run_gh;
use natamux_proto::tasks::task::Task;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

use super::TaskStore;
use crate::agents::generic_signals::GenericSignals;
use crate::agents::idle_pause::{IdlePause, IdlePauseOptions, PausableShells};
use crate::agents::runner::AgentRunner;
use crate::config::Config;
use crate::github::issue_importer::{IssueImporter, IssueImporterOptions};
use crate::github::issue_linker::{IssueLinker, IssueLinkerOptions, LinkError};
use crate::github::pull_request_watcher::{PullRequestWatcher, PullRequestWatcherOptions};
use crate::projects::repo_trust::TrustedOf;
use crate::projects::repos::{RepoOptions, Repos, TaskRef};
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::{Jobs, system_clock};
use crate::shared::scheduler::Scheduler;
use crate::stages::runner::StageRunner;
use crate::tasks::errors::TaskError;
use crate::tasks::service::{TaskService, TaskServiceOptions, is_task_ended};
use crate::terminals::limits::MAIN_TERMINAL;
use crate::terminals::open_shells::OpenShells;
use crate::terminals::service::TerminalService;
use crate::terminals::types::DEFAULT_SIZE;
use crate::workspaces::agent_once::default_run_agent_once;
use crate::workspaces::errors::WorkspaceError;
use crate::workspaces::service::{WorkspaceOptions, WorkspaceService};
use crate::workspaces::setup_script::{SETUP_SKIPPED, WORKSPACE_WARNING_EVENT, setup_command};

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
    let (trust, logging) = (Arc::clone(&store.repos), store.tasks.clone());
    options.on_created = Some(Arc::new(move |task: &Task| {
        run_setup(task, &typing, &trust, &logging);
    }));
    let programs = Arc::clone(agents);
    options.program_of = Arc::new(move |agent: &str| programs.program_of(agent));
    let linking = Arc::clone(linker);
    options.pull_request_body =
        Arc::new(move |task: &Task| linking.pull_request_body(task).map_err(link_failure));
    options.run_agent_once = default_run_agent_once();
    options.trust_paths = Some(agents.trust_paths().clone());
    WorkspaceService::new(options)
}

/// Types the setup script of a new worktree into its first terminal. A project whose commands
/// are not approved gets a line in the task's log instead.
fn run_setup(task: &Task, terminals: &TerminalService, repos: &Repos, tasks: &TaskService) {
    let Some(workspace) = task.workspace.as_deref().map(Path::new) else {
        return;
    };
    let Some(command) = setup_command(workspace, cfg!(windows)) else {
        return;
    };
    if !repos.is_trusted_in(task.repo.as_deref(), workspace) {
        let payload = json!({ "message": SETUP_SKIPPED });
        let logged = tasks.record_event(&task.id, WORKSPACE_WARNING_EVENT, payload, "system");
        if let Err(error) = logged {
            let id = &task.id;
            eprintln!("The skipped setup script of {id} is not logged: {error}");
        }
        return;
    }
    // A failed setup run is not a failed workspace.
    let _ = terminals.run(&task.id, &command, MAIN_TERMINAL, DEFAULT_SIZE);
}

/// Whether a project's commands are approved, asked at each use.
pub(super) fn trusted_of(repos: &Arc<Repos>) -> TrustedOf {
    let repos = Arc::clone(repos);
    Arc::new(move |repo| repos.is_trusted(repo))
}

/// An issue that could not be opened for a pull request: 409 as Node, or the task's own error.
fn link_failure(error: LinkError) -> WorkspaceError {
    match error {
        LinkError::Task(error) => WorkspaceError::Task(error),
        LinkError::GitHub(error) => WorkspaceError::GitHub(error.message),
        LinkError::Link(error) => WorkspaceError::Conflict(error.to_string()),
    }
}

impl PausableShells for TerminalService {
    fn watched(&self, task_id: &str, terminal: &str) -> bool {
        TerminalService::watched(self, task_id, terminal)
    }

    fn pause_one(&self, task_id: &str, terminal: &str, minutes: u64) -> bool {
        TerminalService::pause_one(self, task_id, terminal, minutes)
    }
}

/// The idle pause, which ends the shell of an idle agent that no one looks at.
pub(super) fn idle_pause(
    config: &Config,
    store: &TaskStore,
    terminals: &TerminalService,
    open_shells: &Arc<OpenShells>,
    stages: &StageRunner,
    scheduler: &Arc<Scheduler>,
) -> Arc<IdlePause> {
    let (tasks, recording) = (store.tasks.clone(), store.tasks.clone());
    let settings = Arc::clone(&store.repos);
    let (runs, forgetting) = (stages.clone(), stages.clone());
    IdlePause::new(IdlePauseOptions {
        scheduler: Arc::clone(scheduler),
        minute_ms: config.idle_minute_ms,
        shells: Arc::new(terminals.clone()),
        open_shells: Arc::clone(open_shells),
        task_of: Arc::new(move |task_id: &str| {
            if is_task_ended(&tasks, task_id) {
                return None;
            }
            tasks.get_task(task_id).ok()
        }),
        minutes_of: Arc::new(move |task: &Task| {
            settings
                .settings_of(task.repo.as_deref())
                .agents
                .pause_idle_minutes()
        }),
        agent_runs: Arc::new(move |task_id: &str, terminal: &str| {
            runs.run.agent_runs(task_id, terminal)
        }),
        on_paused: Arc::new(move |task_id: &str, terminal: &str, payload: Value| {
            forgetting.run.forget_shell(task_id, terminal);
            let logged = recording.record_event(task_id, "agent.paused", payload, "system");
            if let Err(error) = logged {
                eprintln!("The pause of terminal {terminal} of {task_id} is not logged: {error}");
            }
        }),
    })
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
    repos.set_trust_bus(Arc::clone(&bus));
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
