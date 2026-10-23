//! Builds the daemon and starts its server (`main.ts`, the parts that exist in Rust so far).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use natamux_pty::Env;
use natamux_store::task_index::TaskIndex;
use tokio::task::JoinHandle;

use crate::config::{Config, shell_env};
use crate::http::server::{AppState, bind, serve};
use crate::projects::repos::Repos;
use crate::shared::event_bus::{DaemonMessage, EventBus};
use crate::shared::jobs::{Jobs, system_clock};
use crate::shared::scheduler::Scheduler;
use crate::tasks::errors::TaskError;
use crate::tasks::service::{TaskService, TaskServiceOptions, is_task_ended};
use crate::tasks::templates::TemplateService;
use crate::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use crate::terminals::service::{TerminalOptions, TerminalService};
use crate::terminals::session_lifecycle::SpawnPty;
use crate::terminals::types::TerminalError;

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

fn io_error(error: TaskError) -> std::io::Error {
    std::io::Error::other(error.to_string())
}

/// The task store of the data folder, loaded, and what shares it.
struct TaskStore {
    tasks: TaskService,
    repos: Arc<Repos>,
    jobs: Arc<Jobs>,
    bus: Arc<EventBus>,
}

fn task_store(config: &Config) -> std::io::Result<TaskStore> {
    let index =
        TaskIndex::open(&config.data_dir.join("index.sqlite")).map_err(std::io::Error::other)?;
    let bus = EventBus::new();
    let jobs = Arc::new(Jobs::new(Some(Arc::clone(&bus)), system_clock()));
    let repos = Repos::new(&config.data_dir, system_clock());
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
    Ok(TaskStore {
        tasks,
        repos,
        jobs,
        bus,
    })
}

/// `env`: the daemon's own environment, the base of the shells' one.
pub async fn start(
    config: &Config,
    env: &Env,
    spawn: Arc<SpawnPty>,
    scheduler: Arc<Scheduler>,
) -> std::io::Result<Daemon> {
    std::fs::create_dir_all(&config.data_dir)?;
    let TaskStore {
        tasks,
        repos,
        jobs,
        bus,
    } = task_store(config)?;
    let listener = bind(&config.host, config.port).await?;
    let address = listener.local_addr()?;
    // Shells run on the same machine as the daemon, so they reach it on loopback.
    let daemon_url = format!("http://127.0.0.1:{}", address.port());
    let ended = tasks.clone();
    let terminals = TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(workspace_of(tasks.clone())),
        env: shell_env(env, &config.agent_token, &daemon_url),
        scheduler: Arc::clone(&scheduler),
        on_output: None,
        is_ended: Some(Arc::new(move |task_id: &str| {
            is_task_ended(&ended, task_id)
        })),
        bus: Some(Arc::clone(&bus)),
        on_output_idle: None,
    });
    let ended = tasks.clone();
    let open_shells = OpenShells::new(OpenShellsOptions {
        data_dir: config.data_dir.clone(),
        terminals: Arc::new(terminals.clone()),
        bus: Arc::clone(&bus),
        scheduler: Arc::clone(&scheduler),
        is_ended: Arc::new(move |task_id| is_task_ended(&ended, task_id)),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    open_shells.start();
    restore_open_shells(&tasks, &open_shells);
    // A deleted task keeps nothing: its shells end with no last screen (Node's `finish`).
    let closing = terminals.clone();
    bus.subscribe(move |message| {
        if let DaemonMessage::TaskDeleted { task_id } = message {
            closing.close(task_id);
        }
    });
    let state = Arc::new(AppState {
        token: config.token.clone(),
        agent_token: config.agent_token.clone(),
        data_dir: config.data_dir.clone(),
        terminals,
        open_shells,
        silent_close: config.silent_close,
        templates: TemplateService::new(&config.data_dir, tasks.clone()),
        tasks,
        repos,
        jobs,
        bus,
        scheduler: Arc::clone(&scheduler),
    });
    let server = tokio::spawn(serve(listener, Arc::clone(&state)));
    Ok(Daemon {
        state,
        scheduler,
        address,
        server,
    })
}

/// The shells that were open come back once a dashboard opens their task (`loadOpenShells`). A
/// task done, cancelled or without a workspace keeps none. Which shells run agents is the stage
/// engine's to say (part 3): until then every shell waits for a dashboard.
fn restore_open_shells(tasks: &TaskService, open_shells: &OpenShells) {
    for task in tasks.list_tasks(None).unwrap_or_default() {
        if task.state.is_finished() || task.workspace.is_none() {
            open_shells.forget(&task.id);
        } else {
            open_shells.load(&task.id, &[]);
        }
    }
}

impl Daemon {
    /// Before the process exits: the list of open shells is not written again, so the shells
    /// the shutdown ends come back at the next start. Waits until they ended.
    pub fn shut_down(&self) {
        self.state.open_shells.stop();
        self.state.terminals.close_all_and_wait().wait();
        self.state.tasks.close();
        self.server.abort();
    }
}
