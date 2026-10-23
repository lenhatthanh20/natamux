//! Builds the daemon of part 1 and starts its server (`main.ts`, the terminal part of it).

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use natamux_pty::Env;
use tokio::task::JoinHandle;

use crate::config::{Config, shell_env};
use crate::http::server::{AppState, bind, serve};
use crate::shared::event_bus::EventBus;
use crate::shared::scheduler::Scheduler;
use crate::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use crate::terminals::service::{TerminalOptions, TerminalService};
use crate::terminals::session_lifecycle::SpawnPty;
use crate::terminals::shell_files::tasks_with_open_shells;
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

/// Without the task store (part 2), a task is its worktree folder: `<worktrees>/<id>`.
pub fn worktree_of(worktrees_dir: PathBuf) -> impl Fn(&str) -> Result<PathBuf, TerminalError> {
    move |task_id| {
        let folder = worktrees_dir.join(task_id);
        if natamux_proto::tasks::task::is_task_id(task_id) && folder.is_dir() {
            Ok(folder)
        } else {
            Err(TerminalError::NoWorkspace(task_id.into()))
        }
    }
}

/// `env`: the daemon's own environment, the base of the shells' one.
pub async fn start(
    config: &Config,
    env: &Env,
    spawn: Arc<SpawnPty>,
    scheduler: Arc<Scheduler>,
) -> std::io::Result<Daemon> {
    std::fs::create_dir_all(&config.data_dir)?;
    let listener = bind(&config.host, config.port).await?;
    let address = listener.local_addr()?;
    // Shells run on the same machine as the daemon, so they reach it on loopback.
    let daemon_url = format!("http://127.0.0.1:{}", address.port());
    let bus = EventBus::new();
    let terminals = TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(worktree_of(config.worktrees_dir.clone())),
        env: shell_env(env, &config.agent_token, &daemon_url),
        scheduler: Arc::clone(&scheduler),
        on_output: None,
        is_ended: None,
        bus: Some(Arc::clone(&bus)),
        on_output_idle: None,
    });
    let open_shells = OpenShells::new(OpenShellsOptions {
        data_dir: config.data_dir.clone(),
        terminals: Arc::new(terminals.clone()),
        bus,
        scheduler: Arc::clone(&scheduler),
        is_ended: Arc::new(|_| false),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    open_shells.start();
    // The shells that were open come back once a dashboard opens their task. A task whose folder
    // is gone keeps none.
    let workspace_of = worktree_of(config.worktrees_dir.clone());
    for task_id in tasks_with_open_shells(&config.data_dir) {
        if workspace_of(&task_id).is_ok() {
            open_shells.load(&task_id, &[]);
        } else {
            open_shells.forget(&task_id);
        }
    }
    let state = Arc::new(AppState {
        token: config.token.clone(),
        agent_token: config.agent_token.clone(),
        data_dir: config.data_dir.clone(),
        terminals,
        open_shells,
        silent_close: config.silent_close,
    });
    let server = tokio::spawn(serve(listener, Arc::clone(&state)));
    Ok(Daemon {
        state,
        scheduler,
        address,
        server,
    })
}

impl Daemon {
    /// Before the process exits: the list of open shells is not written again, so the shells
    /// the shutdown ends come back at the next start. Waits until they ended.
    pub fn shut_down(&self) {
        self.state.open_shells.stop();
        self.state.terminals.close_all_and_wait().wait();
        self.server.abort();
    }
}
