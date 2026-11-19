//! The hook bench on the wiring of `start.rs`: the stage runner finds the lane, the stages come
//! from the project's `.natamux.json`, a dashboard listens on the event bus, the open shells keep
//! each session and the usage service hears each turn end. Each agent's task has 500 events and
//! the store holds 2,000 finished tasks too. Reports only; no targets here.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use natamux_agents::hook_files::WorkspaceHooks;
use natamux_agents::hooks::Signal;
use natamux_agents::trust::trust_paths;
use natamux_daemon::agents::hook_service::{HookOptions, HookService, StageLookup};
use natamux_daemon::agents::runner::{AgentRunner, AgentRunnerOptions};
use natamux_daemon::agents::usage_service::{UsageOptions, UsageService};
use natamux_daemon::projects::repos::{RepoOptions, Repos};
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::system_clock;
use natamux_daemon::shared::scheduler::Scheduler;
use natamux_daemon::stages::run::StageRunOptions;
use natamux_daemon::stages::runner::StageRunner;
use natamux_daemon::tasks::records::WorkspaceChange;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions, is_task_ended};
use natamux_daemon::terminals::open_shells::{OpenShells, OpenShellsOptions, ShellsView};
use natamux_proto::stages::catalog::full_pipeline;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_store::task_files::write_task_file;
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

use super::loads::{
    HookBench, HookTimes, PACE, bench_task, temp_dir, times_from_threads, times_single,
};
use super::setup;

const EVENTS_PER_TASK: usize = 500;
const FINISHED_TASKS: usize = 2_000;
/// A lane of a group: the lookup of the terminal's stage reads the pinned terminals.
const STAGE: &str = "implement";
/// As `start.rs`: the usage file is read this long after the turn end, off the hook's thread.
const USAGE_READ_DELAY: Duration = Duration::from_secs(2);

/// The hook times from one thread, then with `agents` agents at the pace of `PACE`.
pub fn staged_hooks(agents: usize) -> (HookTimes, HookTimes) {
    let dir = temp_dir("staged-hooks");
    let (bench, usage) = staged_setup(&dir, agents);
    let single = times_single(&bench);
    let paced = times_from_threads(&bench, Some(PACE));
    usage.settle();
    std::fs::remove_dir_all(&dir).ok();
    (single, paced)
}

/// Every shell of the bench is open, as the shell of a working agent is.
struct AllOpen;

impl ShellsView for AllOpen {
    fn list(&self, _task_id: &str) -> Vec<String> {
        vec!["1".into()]
    }

    fn is_open(&self, _task_id: &str, _terminal: &str) -> bool {
        true
    }

    fn foreground(&self, _task_id: &str, _terminal: &str) -> Option<String> {
        None
    }
}

fn staged_setup(dir: &Path, agents: usize) -> (Arc<HookBench>, Arc<UsageService>) {
    let repo = dir.join("repo");
    std::fs::create_dir_all(&repo).unwrap();
    std::fs::write(repo.join(".natamux.json"), settings_text()).unwrap();
    let data_dir = dir.join("data");
    std::fs::create_dir_all(&data_dir).unwrap();
    for number in 1..=FINISHED_TASKS {
        let task = Task {
            state: TaskState::Done,
            workspace: None,
            ..bench_task(number)
        };
        write_task_file(&data_dir, &task).unwrap();
    }
    let bus = EventBus::new();
    // A dashboard: each message is made into its text, as for an event socket.
    bus.subscribe_client(|text| {
        std::hint::black_box(text.len());
    });
    let repos = Repos::with_options(RepoOptions {
        data_dir: data_dir.clone(),
        default_path: Some(repo),
        worktrees_dir: None,
        tasks: None,
        now: system_clock(),
    })
    .unwrap();
    let stages_of = Arc::clone(&repos);
    let tasks = TaskService::new(TaskServiceOptions {
        data_dir: data_dir.clone(),
        index: TaskIndex::open(&data_dir.join("index.sqlite")).unwrap(),
        bus: Arc::clone(&bus),
        now: None,
        settings_of: Some(Arc::new(move |repo| stages_of.stages_of(repo))),
        clock: None,
    });
    tasks.init().unwrap();
    let home = dir.join("home");
    let ended = tasks.clone();
    let open_shells = OpenShells::new(OpenShellsOptions {
        data_dir: data_dir.clone(),
        terminals: Arc::new(AllOpen),
        bus: Arc::clone(&bus),
        scheduler: Scheduler::real(),
        is_ended: Arc::new(move |task_id| is_task_ended(&ended, task_id)),
        agent_runs: None,
        agent_in: None,
        save: None,
    });
    let runner = stage_runner(&tasks, &repos, &bus, &data_dir, &home, &open_shells);
    let usage = Arc::new(UsageService::new(UsageOptions {
        data_dir,
        bus: Some(Arc::clone(&bus)),
        home: home.clone(),
        read_delay: USAGE_READ_DELAY,
    }));
    let mut workspaces = Vec::new();
    for agent in 0..agents {
        let input = json!({ "title": format!("Bench agent {agent}"), "agent": "claude-code" });
        let task = tasks.create_task(Some(&input), &[], true).unwrap();
        let workspace = format!("/work/staged-{agent}");
        let change = WorkspaceChange {
            workspace: Some(workspace.clone()),
            branch: Some("b".into()),
        };
        let task = tasks.record_workspace(&task.id, change).unwrap();
        seed_events(&tasks, &task.id);
        if agent == 0 {
            check_lane(&runner, &tasks.get_task(&task.id).unwrap());
        }
        workspaces.push(workspace);
    }
    let options = hook_options(&bus, &open_shells, &usage, home.clone());
    let service = Arc::new(HookService::new(tasks, Arc::new(runner), options));
    let bench = HookBench {
        service,
        workspaces,
        sessions: Some(home),
    };
    (Arc::new(bench), usage)
}

/// Not started: a started runner would launch agents and run stage checks in the bench.
fn stage_runner(
    tasks: &TaskService,
    repos: &Arc<Repos>,
    bus: &Arc<EventBus>,
    data_dir: &Path,
    home: &Path,
    open_shells: &Arc<OpenShells>,
) -> StageRunner {
    let (terminals, _) = setup();
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
        trust_paths: trust_paths(|_| None, home),
        permission_mode_of: Arc::new(|_| String::new()),
        trusted_of: Arc::new(|_| true),
    }));
    let (paths, settings) = (Arc::clone(repos), Arc::clone(repos));
    let mut options = StageRunOptions::new(
        tasks.clone(),
        terminals,
        agents,
        Arc::clone(bus),
        data_dir.to_path_buf(),
        Arc::new(move |repo| paths.path_of(repo)),
        Arc::new(move |repo| settings.settings_of(repo)),
    );
    options.home = home.to_path_buf();
    options.open_shells = Some(Arc::clone(open_shells));
    StageRunner::new(options)
}

/// What `start.rs` tells of a hook. The workspace and idle-pause parts only note the shell there.
fn hook_options(
    bus: &Arc<EventBus>,
    open_shells: &Arc<OpenShells>,
    usage: &Arc<UsageService>,
    home: PathBuf,
) -> HookOptions {
    let (noted, seen) = (Arc::clone(open_shells), Arc::clone(usage));
    let touched: Arc<Mutex<HashSet<String>>> = Arc::default();
    let signals: Arc<Mutex<HashMap<String, (Signal, Instant)>>> = Arc::default();
    HookOptions {
        bus: Some(Arc::clone(bus)),
        workspace_changed: Some(Arc::new(move |task_id: &str| {
            touched.lock().unwrap().insert(task_id.to_string());
        })),
        session_seen: Some(Arc::new(move |task: &str, terminal: &str, agent: &str, session: &str| {
            noted.note_session(task, terminal, agent, session);
        })),
        signal_seen: Some(Arc::new(move |task: &str, terminal: &str, signal: Signal| {
            let key = format!("{task}/{terminal}");
            signals.lock().unwrap().insert(key, (signal, Instant::now()));
        })),
        usage_seen: Some(Arc::new(
            move |task: &str, stage: Option<&str>, agent: &str, transcript: &str| {
                seen.seen(task, stage, agent, transcript);
            },
        )),
        home,
    }
}

/// The stage starts in terminal 1, then the agent's waits fill the log; they keep it running.
fn seed_events(tasks: &TaskService, id: &str) {
    let apply = |input: Value| {
        tasks.apply_event(id, Some(&input)).unwrap();
    };
    apply(json!({
        "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": STAGE },
    }));
    apply(json!({
        "source": "system", "type": "stage.started", "payload": { "stage": STAGE, "terminal": "1" },
    }));
    let logged = tasks.list_events(id).unwrap().len();
    for _ in logged..EVENTS_PER_TASK {
        apply(json!({ "source": "agent", "type": "agent.waiting", "payload": { "stage": STAGE } }));
    }
}

/// A bench whose hooks miss the stage measures another path: said in the log, not failed.
fn check_lane(runner: &StageRunner, task: &Task) {
    let lane = runner.lane_for(task, Some("1"));
    if !matches!(&lane, Ok(Some((stage, TaskState::Running))) if stage == STAGE) {
        eprintln!("staged hooks: the agent is not on a running {STAGE} lane: {lane:?}");
    }
}

/// A `.natamux.json` with the full pipeline, each stage with a prompt of a usual length.
fn settings_text() -> String {
    let mut stages = serde_json::to_value(full_pipeline()).unwrap();
    add_prompts(&mut stages);
    json!({ "stages": stages }).to_string()
}

fn add_prompts(value: &mut Value) {
    match value {
        Value::Array(steps) => steps.iter_mut().for_each(add_prompts),
        Value::Object(fields) => match fields.get_mut("parallel") {
            Some(lanes) => add_prompts(lanes),
            None => {
                let prompt = "Read the task, its context and the files of the stages before. \
                              Do the work of this stage, check it, then report it.\n";
                fields.insert("prompt".into(), json!(prompt.repeat(12)));
            }
        },
        _ => {}
    }
}
