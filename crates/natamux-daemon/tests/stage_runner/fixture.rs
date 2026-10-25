//! The `beforeEach` of `stage-runner.test.ts` and its helpers.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex};

use natamux_agents::hook_files::WorkspaceHooks;
use natamux_agents::trust::trust_paths;
use natamux_daemon::agents::runner::{AgentRunner, AgentRunnerOptions};
use natamux_daemon::shared::event_bus::{DaemonMessage, EventBus};
use natamux_daemon::shared::jobs::{Clock, Jobs};
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::shared::slug::slugify;
use natamux_daemon::stages::run::StageRunOptions;
use natamux_daemon::stages::runner::StageRunner;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_daemon::terminals::open_shells::OpenShells;
use natamux_daemon::terminals::service::{TerminalOptions, TerminalService};
use natamux_daemon::terminals::types::TerminalError;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::projects::repo::ProjectSettings;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::Task;
use natamux_stages::checks::{Ran, RunCommand};
use natamux_stages::ci::CiCheck;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

pub use crate::helpers::*;
pub use crate::support::FakePty;
use crate::support::{SpawnGate, fake_spawner_gated};

pub struct Fixture {
    pub dir: TempDir,
    /// The repository: `<dir>/repo`, with a bare origin at `<dir>/origin.git`.
    pub repo: PathBuf,
    pub data_dir: PathBuf,
    pub worktrees: PathBuf,
    pub home: PathBuf,
    pub tasks: TaskService,
    pub terminals: TerminalService,
    pub agents: Arc<AgentRunner>,
    pub runner: StageRunner,
    pub bus: Arc<EventBus>,
    pub messages: Arc<Mutex<Vec<DaemonMessage>>>,
    pub ptys: Arc<Mutex<Vec<FakePty>>>,
    pub settings: Arc<Mutex<ProjectSettings>>,
    pub clock: Arc<AtomicI64>,
    /// What `runCommand` answers; tests replace it.
    pub run_command: Arc<Mutex<RunCommand>>,
    pub gh_calls: Arc<Mutex<Vec<Vec<String>>>>,
    pub ci: Arc<FakeCi>,
    pub jobs: Arc<Jobs>,
    /// Makes the spawn of chosen shells fail (the `vi.spyOn(terminals, 'run')` of Node).
    pub spawn_gate: SpawnGate,
}

fn default_settings() -> ProjectSettings {
    ProjectSettings {
        stages: default_pipeline(),
        ..ProjectSettings::default()
    }
}

impl Fixture {
    pub fn new() -> Fixture {
        let dir = TempDir::new();
        let repo = test_repo(&dir.0);
        let data_dir = dir.0.join("data");
        std::fs::create_dir_all(&data_dir).unwrap();
        let bus = EventBus::new();
        let messages: Arc<Mutex<Vec<DaemonMessage>>> = Arc::default();
        let sink = Arc::clone(&messages);
        bus.subscribe(move |message| sink.lock().unwrap().push(message.clone()));
        let settings = Arc::new(Mutex::new(default_settings()));
        let clock = Arc::new(AtomicI64::new(start_millis()));
        let read = Arc::clone(&clock);
        let now: Clock = Arc::new(move || read.load(Ordering::SeqCst));
        let stages = Arc::clone(&settings);
        let tasks = TaskService::new(TaskServiceOptions {
            data_dir: data_dir.clone(),
            index: TaskIndex::open(&data_dir.join("index.sqlite")).unwrap(),
            bus: Arc::clone(&bus),
            now: None,
            settings_of: Some(Arc::new(move |_| stages.lock().unwrap().stages.clone())),
            clock: None,
        });
        tasks.init().unwrap();
        let spawn_gate = SpawnGate::default();
        let (spawn, ptys) = fake_spawner_gated(Arc::clone(&spawn_gate));
        let terminals = fake_terminals(&tasks, spawn, &bus);
        let home = dir.0.join("home");
        let modes = Arc::clone(&settings);
        let agents = Arc::new(AgentRunner::new(AgentRunnerOptions {
            tasks: tasks.clone(),
            terminals: terminals.clone(),
            claude_settings_path: data_dir.join("settings.json"),
            gemini_settings_path: None,
            workspace_hooks: WorkspaceHooks::default(),
            programs: Default::default(),
            extra_args: Default::default(),
            windows: false,
            launch_dir: None,
            bin_dir: None,
            on_launch: None,
            // Cursor and Copilot trust the workspace in their config, which must not be the real one.
            trust_paths: trust_paths(|_| None, &home),
            permission_mode_of: Arc::new(move |_| {
                modes.lock().unwrap().agents.permission_mode.clone()
            }),
        }));
        let run_command: Arc<Mutex<RunCommand>> =
            Arc::new(Mutex::new(Arc::new(|_: &str, _: &Path| Ran {
                code: 0,
                output: String::new(),
            })));
        let jobs = Arc::new(Jobs::new(Some(Arc::clone(&bus)), Arc::clone(&now)));
        let mut fixture = Fixture {
            worktrees: dir.0.join("worktrees"),
            repo,
            data_dir,
            home,
            runner: StageRunner::new(dummy_options(&tasks, &terminals, &agents, &bus)),
            tasks,
            terminals,
            agents,
            bus,
            messages,
            ptys,
            settings,
            clock,
            run_command,
            gh_calls: Arc::default(),
            ci: Arc::default(),
            jobs,
            spawn_gate,
            dir,
        };
        fixture.runner = fixture.make_runner(None);
        fixture.runner.start();
        fixture
    }

    /// `makeRunner`: a runner of this fixture, with open shells when given.
    pub fn make_runner(&self, open_shells: Option<Arc<OpenShells>>) -> StageRunner {
        let repo = self.repo.to_string_lossy().into_owned();
        let settings = Arc::clone(&self.settings);
        let mut options = StageRunOptions::new(
            self.tasks.clone(),
            self.terminals.clone(),
            Arc::clone(&self.agents),
            Arc::clone(&self.bus),
            self.data_dir.clone(),
            Arc::new(move |_| Ok(repo.clone())),
            Arc::new(move |_| settings.lock().unwrap().clone()),
        );
        let calls = Arc::clone(&self.gh_calls);
        options.gh = Arc::new(move |_: &Path, args: &[String]| {
            calls.lock().unwrap().push(args.to_vec());
            Ok(issue().to_string())
        });
        let run_command = Arc::clone(&self.run_command);
        options.run_command = Arc::new(move |command: &str, cwd: &Path| {
            let current = Arc::clone(&run_command.lock().unwrap());
            current(command, cwd)
        });
        let read = Arc::clone(&self.clock);
        options.now = Arc::new(move || read.load(Ordering::SeqCst));
        options.windows = false;
        // The skills of the machine that runs the tests must not count.
        options.home = self.home.clone();
        options.ci = Arc::clone(&self.ci) as Arc<dyn CiCheck>;
        options.open_shells = open_shells;
        options.jobs = Arc::clone(&self.jobs);
        StageRunner::new(options)
    }

    /// From now on a shell for which `fails(task id, terminal)` holds cannot start.
    pub fn fail_spawn(&self, fails: impl Fn(&str, &str) -> bool + Send + Sync + 'static) {
        *self.spawn_gate.lock().unwrap() = Some(Arc::new(fails));
    }

    pub fn settle(&self) {
        self.runner.settle();
    }

    pub fn set_settings(&self, settings: ProjectSettings) {
        *self.settings.lock().unwrap() = settings;
    }

    pub fn set_run_command(&self, run: impl Fn(&str, &Path) -> Ran + Send + Sync + 'static) {
        *self.run_command.lock().unwrap() = Arc::new(run);
    }

    /// The default settings with these steps (JSON) in place of the pipeline.
    pub fn set_stages(&self, stages: Value) {
        self.set_settings(ProjectSettings {
            stages: serde_json::from_value(stages).unwrap(),
            ..ProjectSettings::default()
        });
    }

    /// `tasks.setSkip`, then the runner settles.
    pub fn set_skip(&self, id: &str, stage: &str, on: bool) {
        self.tasks.set_skip(id, stage, on).unwrap();
        self.bus.deliver_all();
        self.settle();
    }

    /// `tasks.setCi`.
    pub fn set_ci(&self, id: &str, stage: &str, on: bool) {
        self.tasks.set_ci(id, stage, on).unwrap();
        self.bus.deliver_all();
        self.settle();
    }

    /// The logged events of a task, as `tasks.listEvents(id).map(e => e.event)`.
    pub fn task_events(&self, id: &str) -> Vec<TaskEvent> {
        self.tasks
            .list_events(id)
            .unwrap()
            .into_iter()
            .map(|stored| stored.event)
            .collect()
    }

    /// The last event of this type.
    pub fn last_of(&self, id: &str, kind: &str) -> Option<TaskEvent> {
        self.task_events(id)
            .into_iter()
            .rev()
            .find(|event| event.kind == kind)
    }

    /// Moves the clock by `ms`.
    pub fn advance(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    pub fn ptys(&self) -> Vec<FakePty> {
        self.ptys.lock().unwrap().clone()
    }

    pub fn pty(&self, index: usize) -> FakePty {
        self.ptys()[index].clone()
    }

    /// `created`: a new task from the input, once the runner settled.
    pub fn created(&self, input: Value) -> Task {
        let task = self.tasks.create_task(Some(&input), &[], true).unwrap();
        self.settle();
        self.tasks.get_task(&task.id).unwrap()
    }

    /// `withWorkspace`: a worktree on the task's branch (kept when it exists), from its parent's
    /// branch or main, as `WorkspaceService.create` makes it.
    pub fn with_workspace(&self, id: &str) -> Task {
        let task = self.tasks.get_task(id).unwrap();
        let path = self.worktrees.join(id);
        std::fs::create_dir_all(&self.worktrees).unwrap();
        let path_text = path.to_string_lossy().into_owned();
        let branches = run_git(&self.repo, &["branch", "--format=%(refname:short)"]);
        let has = |name: &str| branches.lines().any(|line| line == name);
        let branch = match &task.branch {
            Some(kept) if has(kept) => {
                run_git(&self.repo, &["worktree", "add", &path_text, kept]);
                kept.clone()
            }
            _ => {
                let branch = format!("natamux/{}-{}", id.to_lowercase(), slugify(&task.title));
                let parent = task
                    .parent_id
                    .as_ref()
                    .and_then(|parent| self.tasks.get_task(parent).ok()?.branch)
                    .filter(|parent| has(parent));
                let from = parent.unwrap_or_else(|| "main".into());
                run_git(
                    &self.repo,
                    &["worktree", "add", "-b", &branch, &path_text, &from],
                );
                branch
            }
        };
        self.tasks
            .record_workspace(
                id,
                WorkspaceChange {
                    workspace: Some(path_text),
                    branch: Some(branch),
                },
            )
            .unwrap();
        self.settle();
        self.tasks.get_task(id).unwrap()
    }

    /// The workspace goes, its branch stays (`workspaces.remove`).
    pub fn remove_workspace(&self, id: &str) {
        let task = self.tasks.get_task(id).unwrap();
        self.terminals.close(id);
        if let Some(workspace) = &task.workspace {
            // `WorkspaceService.remove` copies the stage files after the shells ended.
            natamux_stages::files::archive_stage_files(&self.data_dir, id, Some(workspace))
                .unwrap();
            run_git(&self.repo, &["worktree", "remove", "--force", workspace]);
        }
        self.tasks
            .record_workspace(
                id,
                WorkspaceChange {
                    workspace: None,
                    branch: task.branch.clone(),
                },
            )
            .unwrap();
        self.settle();
    }

    /// `addBareOrigin`: a bare repository next to the test repository, as its `origin`.
    pub fn add_bare_origin(&self) -> PathBuf {
        let remote = self.dir.0.join("origin.git");
        let remote_text = remote.to_string_lossy().into_owned();
        run_git(
            &self.dir.0,
            &["init", "-q", "--bare", "-b", "main", &remote_text],
        );
        run_git(&self.repo, &["remote", "add", "origin", &remote_text]);
        remote
    }

    pub fn launch_script(&self, id: &str, terminal: &str) -> String {
        let suffix = if terminal == "1" {
            String::new()
        } else {
            format!("-{terminal}")
        };
        std::fs::read_to_string(self.data_dir.join(format!("agent-launch-{id}{suffix}.sh")))
            .unwrap()
    }

    /// A task of a pipeline that starts with Implement, ready and in its workspace, so the stage
    /// runs (`implementing`).
    pub fn implementing(&self, stage: &str) -> Task {
        let task = self.created(json!({ "title": "Export", "agent": "claude-code" }));
        self.apply(
            &task.id,
            json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": stage } }),
        );
        self.with_workspace(&task.id)
    }

    /// `tasks.applyEvent`, then the runner settles.
    pub fn apply(&self, id: &str, input: Value) -> natamux_daemon::tasks::store::Applied {
        let applied = self.tasks.apply_event(id, Some(&input)).unwrap();
        self.bus.deliver_all();
        self.settle();
        applied
    }

    pub fn task(&self, id: &str) -> Task {
        self.tasks.get_task(id).unwrap()
    }

    /// The (type, payload) of each logged event of a task.
    pub fn events(&self, id: &str) -> Vec<(String, Value)> {
        self.tasks
            .list_events(id)
            .unwrap()
            .into_iter()
            .map(|stored| (stored.event.kind, Value::Object(stored.event.payload)))
            .collect()
    }

    /// The job lists of the `jobs.changed` messages so far.
    pub fn job_lists(&self) -> Vec<Vec<Job>> {
        self.messages
            .lock()
            .unwrap()
            .iter()
            .filter_map(|message| match message {
                DaemonMessage::JobsChanged { jobs } => Some(jobs.clone()),
                _ => None,
            })
            .collect()
    }
}

/// `createTestRepo`: a git repository with one empty commit on main, and its own identity.
fn test_repo(root: &Path) -> PathBuf {
    let repo = root.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    for (key, value) in [
        ("user.name", "Test"),
        ("user.email", "test@example.com"),
        ("commit.gpgsign", "false"),
        ("core.autocrlf", "false"),
    ] {
        run_git(&repo, &["config", key, value]);
    }
    run_git(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
    repo
}

/// Terminals with fake ptys in the task's workspace.
fn fake_terminals(
    tasks: &TaskService,
    spawn: Arc<natamux_daemon::terminals::session_lifecycle::SpawnPty>,
    bus: &Arc<EventBus>,
) -> TerminalService {
    let workspaces = tasks.clone();
    TerminalService::new(TerminalOptions {
        spawn,
        workspace_of: Arc::new(move |task_id: &str| match workspaces.get_task(task_id) {
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
        trace_input: false,
    })
}

/// Options for the first runner, replaced at once by `make_runner`.
fn dummy_options(
    tasks: &TaskService,
    terminals: &TerminalService,
    agents: &Arc<AgentRunner>,
    bus: &Arc<EventBus>,
) -> StageRunOptions {
    StageRunOptions::new(
        tasks.clone(),
        terminals.clone(),
        Arc::clone(agents),
        Arc::clone(bus),
        PathBuf::new(),
        Arc::new(|_| Ok(String::new())),
        Arc::new(|_| default_settings()),
    )
}
