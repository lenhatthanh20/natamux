//! Builds the daemon and starts its server (`main.ts`, the parts that exist in Rust so far).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use natamux_agents::hook_files::{
    WorkspaceHooks, gemini_hook_settings, hook_file_text_of, prepare_agent_hooks,
};
use natamux_agents::trust::trust_paths;
use natamux_core::agents::hook_settings::claude_code_hook_settings;
use natamux_pty::{Env, SpawnOptions};
use natamux_stages::ci_check::GhCi;
use tokio::task::JoinHandle;

use crate::agents::generic_signals::GenericSignals;
use crate::agents::hook_service::{HookOptions, HookService};
use crate::agents::runner::{AgentLaunch, AgentRunner, AgentRunnerOptions};
use crate::agents::usage_service::{UsageOptions, UsageService};
use crate::config::{Config, shell_env};
use crate::http::server::{AppState, bind, serve};
use crate::http::web::WebAssets;
use crate::projects::repos::Repos;
use crate::shared::event_bus::EventBus;
use crate::shared::jobs::Jobs;
use crate::shared::scheduler::Scheduler;
use crate::stages::run::StageRunOptions;
use crate::stages::runner::StageRunner;
use crate::tasks::errors::TaskError;
use crate::tasks::service::{TaskService, is_task_ended};
use crate::tasks::templates::TemplateService;
use crate::terminals::input_trace::trace_input_on;
use crate::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use crate::terminals::service::{TerminalOptions, TerminalService};
use crate::terminals::session_lifecycle::SpawnPty;
use crate::terminals::types::TerminalError;
use crate::workspaces::service::WorkspaceService;

#[path = "start_services.rs"]
mod services;

use services::{github_services, task_store, workspace_service};

pub struct Daemon {
    pub state: Arc<AppState>,
    pub scheduler: Arc<Scheduler>,
    pub address: SocketAddr,
    pub server: JoinHandle<std::io::Result<()>>,
}

impl ShellsView for TerminalService {
    fn list(&self, task_id: &str) -> Vec<String> {
        TerminalService::list(self, task_id)
    }

    fn is_open(&self, task_id: &str, terminal: &str) -> bool {
        TerminalService::is_open(self, task_id, terminal)
    }

    fn foreground(&self, task_id: &str, terminal: &str) -> Option<String> {
        TerminalService::foreground(self, task_id, terminal)
    }
}

/// The task's workspace (`workspaceOf`): 404 for no task, 409 for a task without one.
pub fn workspace_of(tasks: TaskService) -> impl Fn(&str) -> Result<PathBuf, TerminalError> {
    move |task_id| match tasks.get_task(task_id) {
        Ok(task) => task
            .workspace
            .map(PathBuf::from)
            .ok_or_else(|| TerminalError::NoWorkspace(task_id.into())),
        Err(TaskError::NotFound(_)) => Err(TerminalError::TaskNotFound(task_id.into())),
        Err(error) => Err(TerminalError::Spawn(error.to_string())),
    }
}

/// The user's home folder: where agents keep their sessions.
fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map_or_else(PathBuf::new, PathBuf::from)
}

/// The home folder in the daemon's own environment, where its hooks may read session files.
fn home_of(env: &Env) -> PathBuf {
    env.get("HOME")
        .or_else(|| env.get("USERPROFILE"))
        .map_or_else(PathBuf::new, PathBuf::from)
}

/// An agent the user starts by hand in a shell finds its hooks there: the files are written at
/// each shell start, and only there, not on the output path.
fn with_hook_files(spawn: Arc<SpawnPty>) -> Arc<SpawnPty> {
    Arc::new(move |options: SpawnOptions| {
        prepare_agent_hooks(&options.cwd, WorkspaceHooks { claude_code: true });
        (*spawn)(options)
    })
}

/// The task store of the data folder, loaded, and what shares it.
struct TaskStore {
    tasks: TaskService,
    repos: Arc<Repos>,
    jobs: Arc<Jobs>,
    bus: Arc<EventBus>,
    /// What output alone tells of a task (bell, sign of life, a shell that lacks the agent).
    signals: Arc<GenericSignals>,
}

/// Late links between parts that need each other: the terminals call the stage runner on output,
/// the open shells ask it which shells run agents.
type RunnerCell = Arc<OnceLock<StageRunner>>;
/// The workspace service is built last (it needs the terminals and the agents), but the terminals
/// and the hooks tell it of output and file changes.
type WorkspaceCell = Arc<OnceLock<Arc<WorkspaceService>>>;

#[derive(Default)]
struct Cells {
    runner: RunnerCell,
    workspaces: WorkspaceCell,
}

fn agent_runner(
    config: &Config,
    tasks: &TaskService,
    terminals: &TerminalService,
    repos: &Arc<Repos>,
    open_shells: &Arc<OpenShells>,
    signals: &Arc<GenericSignals>,
    daemon_url: &str,
) -> std::io::Result<AgentRunner> {
    // Shells reach the daemon on loopback, so do the hooks of the agents they start.
    let claude_settings_path = config.data_dir.join("claude-code-settings.json");
    std::fs::write(
        &claude_settings_path,
        hook_file_text_of(&claude_code_hook_settings(daemon_url)),
    )?;
    let gemini_settings_path = config.data_dir.join("gemini-settings.json");
    std::fs::write(
        &gemini_settings_path,
        hook_file_text_of(&gemini_hook_settings()),
    )?;
    let home = home_dir();
    let settings = Arc::clone(repos);
    let open = Arc::clone(open_shells);
    let signals = Arc::clone(signals);
    Ok(AgentRunner::new(AgentRunnerOptions {
        tasks: tasks.clone(),
        terminals: terminals.clone(),
        claude_settings_path,
        gemini_settings_path: Some(gemini_settings_path),
        workspace_hooks: WorkspaceHooks { claude_code: true },
        programs: config.agents.programs.clone(),
        extra_args: config.agents.extra_args.clone(),
        windows: cfg!(windows),
        launch_dir: None,
        bin_dir: config.agents.bin_dir.clone(),
        on_launch: Some(Arc::new(move |launch: &AgentLaunch| {
            signals.watch_launch(launch);
            open.note_launch(
                &launch.task_id,
                &launch.terminal,
                &launch.agent,
                launch.session.as_deref(),
            );
        })),
        trust_paths: trust_paths(|name| std::env::var(name).ok(), &home),
        permission_mode_of: Arc::new(move |repo| settings.settings_of(repo).agents.permission_mode),
    }))
}

fn stage_runner(
    config: &Config,
    store: &TaskStore,
    terminals: &TerminalService,
    agents: Arc<AgentRunner>,
    open_shells: &Arc<OpenShells>,
) -> StageRunner {
    let paths = Arc::clone(&store.repos);
    let settings = Arc::clone(&store.repos);
    let mut options = StageRunOptions::new(
        store.tasks.clone(),
        terminals.clone(),
        agents,
        Arc::clone(&store.bus),
        config.data_dir.clone(),
        Arc::new(move |repo| paths.path_of(repo)),
        Arc::new(move |repo| settings.settings_of(repo)),
    );
    options.open_shells = Some(Arc::clone(open_shells));
    options.jobs = Arc::clone(&store.jobs);
    options.ci = Arc::new(GhCi::default());
    StageRunner::new(options)
}

/// The terminals, which tell the stage runner of their output.
fn terminal_service(
    config: &Config,
    env: &Env,
    spawn: Arc<SpawnPty>,
    scheduler: &Arc<Scheduler>,
    store: &TaskStore,
    daemon_url: &str,
    cells: &Cells,
) -> TerminalService {
    let output = Arc::clone(&cells.runner);
    let idle = Arc::clone(&cells.workspaces);
    let signalled = Arc::clone(&store.signals);
    let ended = store.tasks.clone();
    TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(workspace_of(store.tasks.clone())),
        env: shell_env(env, &config.agent_token, daemon_url),
        scheduler: Arc::clone(scheduler),
        on_output: Some(Arc::new(
            move |task_id: &str, terminal: &str, data: &[u8]| {
                if let Some(runner) = output.get() {
                    runner.run.on_output(task_id, terminal, data);
                }
                signalled.on_output(task_id, terminal, data);
            },
        )),
        is_ended: Some(Arc::new(move |task_id: &str| {
            is_task_ended(&ended, task_id)
        })),
        bus: Some(Arc::clone(&store.bus)),
        // Only marks the workspace and plans a timer: no git on the output path.
        on_output_idle: Some(Arc::new(move |task_id: &str| {
            if let Some(workspaces) = idle.get() {
                workspaces.touch(task_id);
            }
        })),
        trace_input: trace_input_on(env.get("NATAMUX_INPUT_TRACE").map(String::as_str)),
    })
}

/// The open shells, which ask the stage runner which shells run agents.
fn open_shells(
    config: &Config,
    store: &TaskStore,
    terminals: &TerminalService,
    scheduler: &Arc<Scheduler>,
    runner: &RunnerCell,
) -> Arc<OpenShells> {
    let ended = store.tasks.clone();
    let (runs, agent_in) = (Arc::clone(runner), Arc::clone(runner));
    OpenShells::new(OpenShellsOptions {
        data_dir: config.data_dir.clone(),
        terminals: Arc::new(terminals.clone()),
        bus: Arc::clone(&store.bus),
        scheduler: Arc::clone(scheduler),
        is_ended: Arc::new(move |task_id| is_task_ended(&ended, task_id)),
        agent_runs: Some(Arc::new(move |task_id: &str, terminal: &str| {
            runs.get()
                .is_some_and(|runner| runner.run.agent_runs(task_id, terminal))
        })),
        agent_in: Some(Arc::new(move |task_id: &str, terminal: &str| {
            agent_in
                .get()
                .and_then(|runner| runner.run.agent_in(task_id, terminal))
        })),
        save: None,
    })
}

/// The hook service and the usage service it tells of each turn.
fn hook_services(
    config: &Config,
    env: &Env,
    store: &TaskStore,
    stages: &StageRunner,
    open_shells: &Arc<OpenShells>,
    workspaces: &WorkspaceCell,
) -> (Arc<HookService>, Arc<UsageService>) {
    let home = home_of(env);
    let usage = Arc::new(UsageService::new(UsageOptions {
        data_dir: config.data_dir.clone(),
        bus: Some(Arc::clone(&store.bus)),
        home: home.clone(),
    }));
    let noted = Arc::clone(open_shells);
    let seen = Arc::clone(&usage);
    let changed = Arc::clone(workspaces);
    let hooks = Arc::new(HookService::new(
        store.tasks.clone(),
        Arc::new(stages.clone()),
        HookOptions {
            bus: Some(Arc::clone(&store.bus)),
            workspace_changed: Some(Arc::new(move |task_id: &str| {
                if let Some(workspaces) = changed.get() {
                    workspaces.touch(task_id);
                }
            })),
            session_seen: Some(Arc::new(
                move |task: &str, terminal: &str, agent: &str, session: &str| {
                    noted.note_session(task, terminal, agent, session);
                },
            )),
            usage_seen: Some(Arc::new(
                move |task: &str, stage: Option<&str>, agent: &str, transcript: &str| {
                    seen.seen(task, stage, agent, transcript);
                },
            )),
            home,
        },
    ));
    (hooks, usage)
}

/// `env`: the daemon's own environment, the base of the shells' one.
pub async fn start(
    config: &Config,
    env: &Env,
    spawn: Arc<SpawnPty>,
    scheduler: Arc<Scheduler>,
) -> std::io::Result<Daemon> {
    std::fs::create_dir_all(&config.data_dir)?;
    let store = task_store(config)?;
    let listener = bind(&config.host, config.port).await?;
    let address = listener.local_addr()?;
    // Shells run on the same machine as the daemon, so they reach it on loopback.
    let daemon_url = format!("http://127.0.0.1:{}", address.port());
    let cells = Cells::default();
    let spawn = with_hook_files(spawn);
    let terminals = terminal_service(config, env, spawn, &scheduler, &store, &daemon_url, &cells);
    let open_shells = open_shells(config, &store, &terminals, &scheduler, &cells.runner);
    let agents = Arc::new(agent_runner(
        config,
        &store.tasks,
        &terminals,
        &store.repos,
        &open_shells,
        &store.signals,
        &daemon_url,
    )?);
    let stages = stage_runner(
        config,
        &store,
        &terminals,
        Arc::clone(&agents),
        &open_shells,
    );
    let _ = cells.runner.set(stages.clone());
    let (issues, issue_linker, pull_requests) = github_services(&store, config, &scheduler);
    let workspaces = workspace_service(
        config,
        &store,
        &scheduler,
        &terminals,
        &agents,
        &issue_linker,
    );
    let _ = cells.workspaces.set(Arc::clone(&workspaces));
    let (hooks, usage) = hook_services(
        config,
        env,
        &store,
        &stages,
        &open_shells,
        &cells.workspaces,
    );
    // Before the listen, so no dashboard attaches to a shell before it is known to wait.
    open_shells.start();
    stages.run.load_open_shells();
    stages.start();
    let built = Built {
        stages: stages.clone(),
        agents,
        hooks,
        usage,
        terminals,
        open_shells,
        workspaces,
        pull_requests,
        issues,
        issue_linker,
    };
    let state = app_state(config, home_of(env), store, &scheduler, built);
    let server = tokio::spawn(serve(listener, Arc::clone(&state)));
    // After the listen, so the hooks of the agents reach the daemon.
    stages.run.background(|run| run.resume_agents());
    Ok(Daemon {
        state,
        scheduler,
        address,
        server,
    })
}

/// The services `start` made, before they go into the `AppState`.
struct Built {
    stages: StageRunner,
    agents: Arc<AgentRunner>,
    hooks: Arc<HookService>,
    usage: Arc<UsageService>,
    terminals: TerminalService,
    open_shells: Arc<OpenShells>,
    workspaces: Arc<WorkspaceService>,
    pull_requests: Arc<crate::github::pull_request_watcher::PullRequestWatcher>,
    issues: Arc<crate::github::issue_importer::IssueImporter>,
    issue_linker: Arc<crate::github::issue_linker::IssueLinker>,
}

fn app_state(
    config: &Config,
    home: PathBuf,
    store: TaskStore,
    scheduler: &Arc<Scheduler>,
    built: Built,
) -> Arc<AppState> {
    let TaskStore {
        tasks,
        repos,
        jobs,
        bus,
        signals: _,
    } = store;
    Arc::new(AppState {
        stages: built.stages,
        agents: built.agents,
        hooks: built.hooks,
        usage: built.usage,
        token: config.token.clone(),
        agent_token: config.agent_token.clone(),
        data_dir: config.data_dir.clone(),
        home,
        web: WebAssets::new(config.web_dir.clone()),
        terminals: built.terminals,
        open_shells: built.open_shells,
        silent_close: config.silent_close,
        templates: TemplateService::new(&config.data_dir, tasks.clone()),
        tasks,
        repos,
        jobs,
        bus,
        scheduler: Arc::clone(scheduler),
        workspaces: built.workspaces,
        pull_requests: built.pull_requests,
        issues: built.issues,
        issue_linker: built.issue_linker,
    })
}

impl Daemon {
    /// Before the process exits: the list of open shells is not written again, so the shells
    /// the shutdown ends come back at the next start. Waits until they ended.
    pub fn shut_down(&self) {
        self.state.pull_requests.stop();
        self.state.issue_linker.stop();
        self.state.open_shells.stop();
        let ended = self.state.terminals.close_all_and_wait();
        self.state.stages.stop();
        ended.wait();
        self.state.tasks.close();
        self.server.abort();
    }
}
