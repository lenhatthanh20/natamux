//! `fan-out.test.ts` and `new-workspace.test.ts`, ported. The fixture is a real git repository,
//! the real task service, project list and workspace service, fake ptys, and (for new workspaces)
//! the real stage runner.
//!
//! Node's new workspace starts its worktree on the same thread until the first `await`, so a test
//! can look at the task before git ran. Here the worktree is made on a thread of its own: a test
//! that looks at that moment holds the workspace service with `hold_create` (its clock waits) and
//! lets it go with `release`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[path = "../support/mod.rs"]
mod support;

mod fan_out;
mod new_workspace;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use natamux_agents::hook_files::WorkspaceHooks;
use natamux_agents::trust::trust_paths;
use natamux_daemon::agents::runner::{AgentRunner, AgentRunnerOptions};
use natamux_daemon::projects::repos::Repos;
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::{Clock, Jobs, system_clock};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::stages::run::StageRunOptions;
use natamux_daemon::stages::runner::StageRunner;
use natamux_daemon::tasks::helpers::legacy_pipeline;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::tasks::store::SettingsOf;
use natamux_daemon::terminals::service::{TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::TerminalError;
use natamux_daemon::workspaces::fan_out::FanOutParts;
use natamux_daemon::workspaces::new_workspace::NewWorkspaceParts;
use natamux_daemon::workspaces::service::{WorkspaceOptions, WorkspaceService};
use natamux_git::test_support::{TempDir, test_repo};
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::tasks::task::Task;
use natamux_store::task_index::TaskIndex;
use serde_json::json;
use support::{FakePty, fake_spawner};

/// `DEFAULT_PROJECT_SETTINGS`: the default pipeline.
pub fn default_settings() -> ProjectSettings {
    ProjectSettings {
        stages: default_pipeline(),
        ..ProjectSettings::default()
    }
}

/// The clock of the workspace service waits here while the gate is closed.
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn set(&self, open: bool) {
        *self.open.lock().unwrap() = open;
        self.changed.notify_all();
    }

    fn pass(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.changed.wait(open).unwrap();
        }
    }
}

pub struct Fx {
    pub repo: PathBuf,
    pub data: PathBuf,
    pub tasks: TaskService,
    pub workspaces: Arc<WorkspaceService>,
    pub agents: Arc<AgentRunner>,
    pub runner: StageRunner,
    pub bus: Arc<EventBus>,
    pub ptys: Arc<Mutex<Vec<FakePty>>>,
    pub log: Arc<Mutex<Vec<String>>>,
    gate: Arc<Gate>,
    with_runner: bool,
    // Last: the folder goes after everything that works in it.
    root: Arc<TempDir>,
}

impl Drop for Fx {
    fn drop(&mut self) {
        self.shutdown();
    }
}

impl Default for Fx {
    fn default() -> Fx {
        Fx::new()
    }
}

impl Fx {
    /// `beforeEach` of `new-workspace.test.ts`: tasks go through the legacy pipeline.
    pub fn new() -> Fx {
        let (root, repo) = test_repo();
        Fx::start(Arc::new(root), repo, None, true)
    }

    /// `beforeEach` of `fan-out.test.ts`: no stage runner, so a task starts no agent by itself.
    pub fn without_runner() -> Fx {
        let (root, repo) = test_repo();
        Fx::start(Arc::new(root), repo, None, false)
    }

    /// `build(settingsOf)`: the daemon starts again on the same folders, with those settings.
    pub fn restart(&mut self, settings: Option<ProjectSettings>) {
        self.shutdown();
        let next = Fx::start(
            Arc::clone(&self.root),
            self.repo.clone(),
            settings,
            self.with_runner,
        );
        *self = next;
    }

    fn start(
        root: Arc<TempDir>,
        repo: PathBuf,
        settings: Option<ProjectSettings>,
        with_runner: bool,
    ) -> Fx {
        let data = root.path().join("data");
        std::fs::create_dir_all(&data).unwrap();
        let home = root.path().join("home");
        std::fs::write(
            data.join("repos.json"),
            json!([{ "id": "default", "name": "default", "path": repo }]).to_string(),
        )
        .unwrap();
        let bus = EventBus::new();
        let task_stages = settings.clone();
        let tasks = TaskService::new(TaskServiceOptions {
            data_dir: data.clone(),
            index: TaskIndex::open(Path::new(":memory:")).unwrap(),
            bus: Arc::clone(&bus),
            now: None,
            settings_of: task_stages.map(|settings| {
                Arc::new(move |_: Option<&str>| settings.stages.clone()) as SettingsOf
            }),
            clock: None,
        });
        tasks.init().unwrap();

        let (spawn, ptys) = fake_spawner();
        let terminals = fake_terminals(&tasks, spawn, &bus);
        let agents = agent_runner(&tasks, &terminals, &data, &home);
        let repo_text = repo.to_string_lossy().into_owned();
        let run_settings = settings.unwrap_or_else(|| ProjectSettings {
            stages: legacy_pipeline(),
            ..ProjectSettings::default()
        });
        let mut stage_options = StageRunOptions::new(
            tasks.clone(),
            terminals,
            Arc::clone(&agents),
            Arc::clone(&bus),
            data.clone(),
            Arc::new(move |_| Ok(repo_text.clone())),
            Arc::new(move |_| run_settings.clone()),
        );
        stage_options.windows = false;
        // The skills of the machine that runs the tests must not count.
        stage_options.home = home;
        let runner = StageRunner::new(stage_options);
        if with_runner {
            runner.start();
        }

        let gate = Arc::new(Gate {
            open: Mutex::new(true),
            changed: Condvar::new(),
        });
        let log: Arc<Mutex<Vec<String>>> = Arc::default();
        let workspaces =
            WorkspaceService::new(workspace_options(&root, &data, &tasks, &bus, &gate, &log));
        Fx {
            repo,
            data,
            tasks,
            workspaces,
            agents,
            runner,
            bus,
            ptys,
            log,
            gate,
            with_runner,
            root,
        }
    }

    fn shutdown(&self) {
        // A test that failed while the worktree was held must not leave its thread waiting.
        self.release();
        // Background git work in the worktree would keep its folder busy on Windows.
        self.settle();
        self.runner.stop();
        self.tasks.close();
    }

    /// `settle` of Node's tests: the worktree, then the stages it started.
    pub fn settle(&self) {
        // A hang fails with a message, not at the CI timeout.
        let until = Instant::now() + Duration::from_secs(30);
        let left = || until.saturating_duration_since(Instant::now());
        for _ in 0..2 {
            if !self.workspaces.settle_for(left()) {
                self.hung("a workspace is still being made");
            }
            self.bus.deliver_all();
            if !self.runner.run.settle_for(left()) {
                self.hung("the stage runner is still running work");
            }
        }
    }

    fn hung(&self, what: &str) -> ! {
        panic!(
            "settle did not finish in 30 s: {what}\npreparing: {:?}\nlog: {:?}",
            self.workspaces.preparing(),
            self.log.lock().unwrap()
        )
    }

    /// The worktree of the next new workspace waits until `release`.
    pub fn hold_create(&self) {
        self.gate.set(false);
    }

    pub fn release(&self) {
        self.gate.set(true);
    }

    pub fn pty_count(&self) -> usize {
        self.ptys.lock().unwrap().len()
    }

    pub fn pty(&self, index: usize) -> FakePty {
        self.ptys.lock().unwrap()[index].clone()
    }

    pub fn task(&self, id: &str) -> Task {
        self.tasks.get_task(id).unwrap()
    }

    pub fn events(&self, id: &str) -> Vec<TaskEvent> {
        self.tasks
            .list_events(id)
            .unwrap()
            .into_iter()
            .map(|stored| stored.event)
            .collect()
    }

    /// The parts of `newWorkspace`: the stage calls go to the real runner.
    pub fn new_parts(&self) -> NewWorkspaceParts {
        let (leave, start) = (Arc::clone(&self.runner.run), Arc::clone(&self.runner.run));
        NewWorkspaceParts {
            tasks: self.tasks.clone(),
            workspaces: Arc::clone(&self.workspaces),
            leave_to_person: Arc::new(move |id| leave.leave_to_person(id)),
            start_stage: Arc::new(move |id| {
                start.act(
                    id,
                    natamux_stages::actions::StageAction::Start,
                    &Default::default(),
                )
            }),
        }
    }

    /// The parts of `fanOut`. Node has no stage runner there: a start by hand goes straight to
    /// the agent.
    pub fn fan_parts(&self) -> FanOutParts {
        let agents = Arc::clone(&self.agents);
        FanOutParts {
            tasks: self.tasks.clone(),
            workspaces: Arc::clone(&self.workspaces),
            start_by_hand: Arc::new(move |id, agent| {
                agents
                    .start(id, agent, None, None, None, &Default::default())
                    .map_err(natamux_daemon::stages::errors::StageError::Agent)
            }),
            data_dir: self.data.clone(),
        }
    }

    /// `startByHand` of the real runner.
    pub fn start_by_hand(&self, id: &str, agent: &str) -> Task {
        self.runner
            .run
            .start_by_hand(id, agent, None, None)
            .unwrap()
    }
}

fn agent_runner(
    tasks: &TaskService,
    terminals: &TerminalService,
    data: &Path,
    home: &Path,
) -> Arc<AgentRunner> {
    Arc::new(AgentRunner::new(AgentRunnerOptions {
        tasks: tasks.clone(),
        terminals: terminals.clone(),
        claude_settings_path: data.join("settings.json"),
        gemini_settings_path: None,
        workspace_hooks: WorkspaceHooks::default(),
        programs: Default::default(),
        extra_args: Default::default(),
        windows: false,
        launch_dir: None,
        bin_dir: None,
        on_launch: None,
        trust_paths: trust_paths(|_| None, home),
        permission_mode_of: Arc::new(|_| "auto".into()),
    }))
}

fn workspace_options(
    root: &TempDir,
    data: &Path,
    tasks: &TaskService,
    bus: &Arc<EventBus>,
    gate: &Arc<Gate>,
    log: &Arc<Mutex<Vec<String>>>,
) -> WorkspaceOptions {
    let mut options = WorkspaceOptions::new(
        data.to_path_buf(),
        root.path().join("worktrees"),
        tasks.clone(),
        Repos::new(data, system_clock()),
        Arc::clone(bus),
        Arc::new(Jobs::new(None, system_clock())),
        Scheduler::manual(),
    );
    let (time, gate) = (AtomicI64::new(0), Arc::clone(gate));
    let clock: Clock = Arc::new(move || {
        gate.pass();
        time.fetch_add(1, Ordering::SeqCst)
    });
    options.now = clock;
    let log = Arc::clone(log);
    options.log = Arc::new(move |message| log.lock().unwrap().push(message.to_string()));
    options
}

fn fake_terminals(
    tasks: &TaskService,
    spawn: Arc<natamux_daemon::terminals::session_lifecycle::SpawnPty>,
    bus: &Arc<EventBus>,
) -> TerminalService {
    let tasks = tasks.clone();
    TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(move |task_id: &str| match tasks.get_task(task_id) {
            Ok(task) => task
                .workspace
                .map(PathBuf::from)
                .ok_or_else(|| TerminalError::NoWorkspace(task_id.into())),
            Err(_) => Err(TerminalError::TaskNotFound(task_id.into())),
        }),
        env: [("PATH", "/bin")]
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect(),
        scheduler: Scheduler::real(),
        on_output: None,
        is_ended: None,
        bus: Some(Arc::clone(bus)),
        on_output_idle: None,
    })
}
