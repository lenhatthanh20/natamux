//! `agent-runner.test.ts`, ported. The workspace is a folder under a plain file, so it cannot be
//! made and no hook file is written there (Node used `/work/T-0001`, which does not exist).
//! Node's `claudeCommand`/`copilotCommand`/`copilotArgs` options are not ported: `programs` and
//! `extra_args` are used in their place.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
#![allow(dead_code)]

#[path = "../support/mod.rs"]
mod support;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use natamux_agents::hook_files::WorkspaceHooks;
use natamux_agents::launch::{Platform, quote_argument};
use natamux_agents::trust::trust_paths;
use natamux_daemon::agents::errors::AgentError;
use natamux_daemon::agents::runner::{
    AgentLaunch, AgentRunner, AgentRunnerOptions, OwnAgent, first_prompt,
};
use natamux_daemon::projects::repos::Repos;
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::Clock;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::terminals::service::{AttachOptions, TerminalOptions, TerminalService};
use natamux_daemon::terminals::session_lifecycle::SpawnPty;
use natamux_daemon::terminals::types::{ClientRef, Size, TerminalError};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::Task;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};
use support::{FakePty, Recorder, fake_spawner};

const SETTINGS: &str = "/data/claude code.json";

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> TempDir {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-agents-{}-{count}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        TempDir(path.canonicalize().unwrap())
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// `beforeEach`.
struct Env {
    dir: TempDir,
    data_dir: PathBuf,
    workspace: String,
    tasks: TaskService,
    terminals: TerminalService,
    ptys: Arc<Mutex<Vec<FakePty>>>,
    runner: AgentRunner,
}

fn terminals_with(
    spawn: Arc<SpawnPty>,
    tasks: &TaskService,
    fixed: Option<&str>,
) -> TerminalService {
    let tasks = tasks.clone();
    let fixed = fixed.map(PathBuf::from);
    TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(move |task_id: &str| {
            if let Some(path) = &fixed {
                return Ok(path.clone());
            }
            match tasks.get_task(task_id) {
                Ok(task) => task
                    .workspace
                    .map(PathBuf::from)
                    .ok_or_else(|| TerminalError::NoWorkspace(task_id.into())),
                Err(_) => Err(TerminalError::TaskNotFound(task_id.into())),
            }
        }),
        env: [("PATH", "/bin")]
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        scheduler: Scheduler::real(),
        on_output: None,
        is_ended: None,
        bus: Some(EventBus::new()),
        on_output_idle: None,
    })
}

fn task_service(data_dir: &Path, clock: Option<Clock>) -> TaskService {
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: data_dir.to_path_buf(),
        index: TaskIndex::open(&data_dir.join("index.sqlite")).unwrap(),
        bus: EventBus::new(),
        now: None,
        settings_of: None,
        clock,
    });
    tasks.init().unwrap();
    tasks
}

impl Env {
    fn new() -> Env {
        let dir = TempDir::new();
        let data_dir = dir.0.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let blocked = dir.0.join("blocked");
        std::fs::write(&blocked, "").unwrap();
        let workspace = blocked.join("work/T-0001").to_string_lossy().into_owned();
        let tasks = task_service(&data_dir, None);
        let (spawn, ptys) = fake_spawner();
        let terminals = terminals_with(spawn, &tasks, None);
        let mut env = Env {
            dir,
            data_dir,
            workspace,
            tasks,
            terminals,
            ptys,
            runner: dummy_runner(),
        };
        env.runner = AgentRunner::new(env.options());
        env
    }

    /// The options of `beforeEach`; a test changes what it needs.
    fn options(&self) -> AgentRunnerOptions {
        AgentRunnerOptions {
            tasks: self.tasks.clone(),
            terminals: self.terminals.clone(),
            claude_settings_path: SETTINGS.into(),
            gemini_settings_path: None,
            workspace_hooks: WorkspaceHooks::default(),
            programs: HashMap::new(),
            extra_args: HashMap::new(),
            windows: false,
            launch_dir: Some(self.data_dir.clone()),
            bin_dir: None,
            on_launch: None,
            trust_paths: trust_paths(|_| None, &self.data_dir),
            permission_mode_of: Arc::new(|_| "auto".into()),
        }
    }

    fn script(&self, task_id: &str) -> PathBuf {
        self.data_dir.join(format!("agent-launch-{task_id}.sh"))
    }

    fn script_text(&self, task_id: &str) -> String {
        std::fs::read_to_string(self.script(task_id)).unwrap()
    }

    fn pty(&self, index: usize) -> FakePty {
        self.ptys.lock().unwrap()[index].clone()
    }

    fn pty_count(&self) -> usize {
        self.ptys.lock().unwrap().len()
    }

    fn task_with_workspace(&self, title: &str, description: &str) -> Task {
        let task = self
            .tasks
            .create_task(
                Some(&json!({ "title": title, "description": description })),
                &[],
                true,
            )
            .unwrap();
        self.tasks
            .record_workspace(
                &task.id,
                WorkspaceChange {
                    workspace: Some(self.workspace.clone()),
                    branch: Some("natamux/t-0001".into()),
                },
            )
            .unwrap()
    }

    fn task(&self) -> Task {
        self.task_with_workspace("Fix the bug", "")
    }

    /// A runner on Windows, with terminals of its own.
    fn windows(
        &self,
        settings: &str,
        bin_dir: Option<&str>,
    ) -> (AgentRunner, Arc<Mutex<Vec<FakePty>>>) {
        let (spawn, ptys) = fake_spawner();
        let terminals = terminals_with(spawn, &self.tasks, Some(&self.workspace));
        let runner = AgentRunner::new(AgentRunnerOptions {
            terminals,
            claude_settings_path: settings.into(),
            windows: true,
            bin_dir: bin_dir.map(String::from),
            ..self.options()
        });
        (runner, ptys)
    }

    fn start(&self, task: &Task, agent: &str, prompt: &str) {
        self.runner
            .start(
                &task.id,
                agent,
                Some(prompt),
                None,
                None,
                &OwnAgent::default(),
            )
            .unwrap();
    }
}

fn dummy_runner() -> AgentRunner {
    let dir = TempDir::new();
    let tasks = task_service(&dir.0, None);
    let (spawn, _) = fake_spawner();
    AgentRunner::new(AgentRunnerOptions {
        terminals: terminals_with(spawn, &tasks, None),
        tasks,
        claude_settings_path: SETTINGS.into(),
        gemini_settings_path: None,
        workspace_hooks: WorkspaceHooks::default(),
        programs: HashMap::new(),
        extra_args: HashMap::new(),
        windows: false,
        launch_dir: None,
        bin_dir: None,
        on_launch: None,
        trust_paths: trust_paths(|_| None, &dir.0),
        permission_mode_of: Arc::new(|_| "auto".into()),
    })
}

/// What Windows types: the command between the window titles that tell its start and its end.
fn windows_line(command: &str) -> String {
    format!(
        "$Host.UI.RawUI.WindowTitle = 'natamux-agent'; {command}; $Host.UI.RawUI.WindowTitle = 'natamux-agent-exited'\r"
    )
}

fn source_line(env: &Env, task: &Task) -> String {
    format!(". '{}'\r", env.script(&task.id).display())
}

fn last_event(tasks: &TaskService, task_id: &str) -> TaskEvent {
    tasks.list_events(task_id).unwrap().pop().unwrap().event
}

fn payload(event: &TaskEvent, key: &str) -> Value {
    event.payload.get(key).cloned().unwrap_or(Value::Null)
}

const PROMPT_ONLY: &str =
    "claude --settings '/data/claude code.json' --permission-mode auto 'Only this'\n";

mod agents;
mod launch;
