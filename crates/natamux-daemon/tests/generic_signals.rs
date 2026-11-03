//! `generic-signals.test.ts`, ported. The signals thread does the work after `on_output`, so each
//! test calls `settle` before it reads the task, where Node's `onOutput` was synchronous.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, AtomicU32, Ordering};

use natamux_core::js::parse_iso_millis;
use natamux_daemon::agents::generic_signals::GenericSignals;
use natamux_daemon::agents::runner::AgentLaunch;
use natamux_daemon::shared::event_bus::EventBus;
use natamux_daemon::shared::jobs::Clock;
use natamux_daemon::tasks::service::{TaskService, TaskServiceOptions};
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_store::task_index::TaskIndex;
use serde_json::{Value, json};

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Fixture {
    dir: TempDir,
    time: Arc<AtomicI64>,
    tasks: TaskService,
    signals: GenericSignals,
}

impl Fixture {
    /// `beforeEach`: 2026-10-08T09:00:00Z, a clock the test moves, shared by the tasks and the
    /// signals.
    fn new() -> Fixture {
        static COUNT: AtomicU32 = AtomicU32::new(0);
        let count = COUNT.fetch_add(1, Ordering::SeqCst);
        let path =
            std::env::temp_dir().join(format!("natamux-generic-{}-{count}", std::process::id()));
        std::fs::create_dir_all(&path).unwrap();
        let dir = TempDir(path);
        let time = Arc::new(AtomicI64::new(
            parse_iso_millis("2026-10-08T09:00:00.000Z").unwrap(),
        ));
        let tasks = Fixture::service(&dir, &time, None);
        let signals = GenericSignals::new(tasks.clone(), Some(Fixture::clock(&time)));
        Fixture {
            dir,
            time,
            tasks,
            signals,
        }
    }

    fn clock(time: &Arc<AtomicI64>) -> Clock {
        let read = Arc::clone(time);
        Arc::new(move || read.load(Ordering::SeqCst))
    }

    fn service(
        dir: &TempDir,
        time: &Arc<AtomicI64>,
        settings: Option<natamux_daemon::tasks::store::SettingsOf>,
    ) -> TaskService {
        TaskService::new(TaskServiceOptions {
            data_dir: dir.0.clone(),
            index: TaskIndex::open(&dir.0.join("index.sqlite")).unwrap(),
            bus: EventBus::new(),
            now: Some(Fixture::clock(time)),
            settings_of: settings,
            clock: None,
        })
    }

    fn later(&self, seconds: i64) {
        self.time.fetch_add(seconds * 1000, Ordering::SeqCst);
    }

    fn running_task(&self, agent: Option<&str>) -> Task {
        let mut input = json!({ "title": "Fix it" });
        if let Some(agent) = agent {
            input["agent"] = json!(agent);
        }
        let task = self.tasks.create_task(Some(&input), &[], true).unwrap();
        self.apply(
            &task.id,
            json!({ "source": "human", "type": "task.readied" }),
        );
        self.apply(
            &task.id,
            json!({ "source": "agent", "type": "agent.started" }),
        );
        task
    }

    fn apply(&self, id: &str, input: Value) {
        self.tasks.apply_event(id, Some(&input)).unwrap();
    }

    /// Output of a terminal, and the wait for what it set going.
    fn output(&self, id: &str, terminal: &str, data: &str) {
        self.signals.on_output(id, terminal, data.as_bytes());
        self.signals.settle();
    }

    fn task(&self, id: &str) -> Task {
        self.tasks.get_task(id).unwrap()
    }
}

#[test]
fn counts_output_of_the_first_terminal_as_a_sign_of_life() {
    let f = Fixture::new();
    let task = f.running_task(Some("cursor"));
    f.later(60);

    f.output(&task.id, "1", "compiling...");

    assert_eq!(f.task(&task.id).updated_at, "2026-10-08T09:01:00.000Z");
}

#[test]
fn keeps_a_running_task_alive_while_it_prints() {
    let f = Fixture::new();
    let task = f.running_task(Some("cursor"));
    f.later(60);
    f.output(&task.id, "1", "a");
    f.later(31);
    f.output(&task.id, "1", "b");
    assert_eq!(f.task(&task.id).updated_at, "2026-10-08T09:01:31.000Z");

    f.later(31);
    f.output(&task.id, "1", "c");
    assert_eq!(f.task(&task.id).updated_at, "2026-10-08T09:02:02.000Z");
}

#[test]
fn keeps_the_wait_for_background_work_while_the_terminal_prints() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.waiting" }),
    );
    let since = f.task(&task.id).background_since;
    f.later(60);

    f.output(&task.id, "1", "\x1b[2J redraw after a resize");

    let after = f.task(&task.id);
    assert_eq!(after.background_since, since);
    assert_eq!(after.updated_at, "2026-10-08T09:01:00.000Z");
}

#[test]
fn moves_a_running_task_to_needs_input_when_an_agent_without_hooks_rings_the_bell() {
    let f = Fixture::new();
    let task = f.running_task(Some("cursor"));

    f.output(&task.id, "1", "Allow this command? \x07");

    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    let last = f.tasks.list_events(&task.id).unwrap().pop().unwrap().event;
    assert_eq!(last.kind, "agent.asked");
    assert_eq!(last.payload["reason"], "terminal bell");
}

#[test]
fn leaves_agents_with_hooks_other_terminals_and_tasks_that_do_not_run_alone() {
    let f = Fixture::new();
    let claude = f.running_task(Some("claude-code"));
    f.output(&claude.id, "1", "\x07");
    assert_eq!(f.task(&claude.id).state, TaskState::Running);

    let other = f.running_task(Some("cursor"));
    f.output(&other.id, "2", "\x07");
    assert_eq!(f.task(&other.id).state, TaskState::Running);

    let draft = f
        .tasks
        .create_task(Some(&json!({ "title": "Not started" })), &[], true)
        .unwrap();
    f.output(&draft.id, "1", "\x07");
    assert_eq!(f.task(&draft.id).state, TaskState::Draft);
}

#[test]
fn asks_for_the_stage_of_the_first_terminal_inside_a_group() {
    let f = Fixture::new();
    let pipeline = default_pipeline();
    let settings: natamux_daemon::tasks::store::SettingsOf = Arc::new(move |_| pipeline.clone());
    let staged = Fixture::service(&f.dir, &f.time, Some(settings));
    let input = json!({ "title": "Export", "agent": "cursor" });
    let task = staged.create_task(Some(&input), &[], true).unwrap();
    let running = json!({
        "source": "human", "type": "state.overridden",
        "payload": { "to": "running", "stage": "build" },
    });
    staged.apply_event(&task.id, Some(&running)).unwrap();

    let signals = GenericSignals::new(staged.clone(), Some(Fixture::clock(&f.time)));
    signals.on_output(&task.id, "1", b"\x07");
    signals.settle();

    let lanes = Value::Object(staged.get_task(&task.id).unwrap().lanes);
    assert_eq!(
        lanes,
        json!({ "test-spec": "running", "implement": "needs-input" })
    );
}

fn launch(f: &Fixture, task_id: &str) {
    f.signals.watch_launch(&AgentLaunch {
        task_id: task_id.to_string(),
        terminal: "1".to_string(),
        agent: "claude-code".to_string(),
        program: "claude".to_string(),
        session: None,
        restored: false,
    });
}

#[test]
fn fails_the_task_with_what_the_shell_said() {
    let lines = [
        "bash: claude: command not found",
        "zsh: command not found: claude",
        "sh: 1: claude: not found",
        "\u{1b}[31mclaude : The term 'claude' is not recognized as the name of a cmdlet\u{1b}[0m",
    ];
    for line in lines {
        let f = Fixture::new();
        let task = f.running_task(Some("claude-code"));
        launch(&f, &task.id);

        f.output(
            &task.id,
            "1",
            &format!("$ . '/data/agent-launch.sh'\r\n{}", &line[..9]),
        );
        f.output(&task.id, "1", &format!("{}\r\n$ ", &line[9..]));

        let failed = f.tasks.list_events(&task.id).unwrap().pop().unwrap().event;
        assert_eq!(f.task(&task.id).state, TaskState::Failed, "{line}");
        assert_eq!(failed.kind, "agent.failed", "{line}");
        let reason = failed.payload["reason"].as_str().unwrap();
        assert!(
            reason.contains("NATAMUX_CLAUDE_COMMAND"),
            "{line}: {reason}"
        );
    }
}

#[test]
fn pays_no_heed_to_other_words_to_other_programs_or_after_the_first_seconds() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    launch(&f, &task.id);
    f.output(&task.id, "1", "bash: pnpm: command not found\r\n");
    f.output(&task.id, "2", "bash: claude: command not found\r\n");
    f.later(20);
    f.output(&task.id, "1", "bash: claude: command not found\r\n");

    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

// The only test of this binary that sets the custom agents (they are the process's).
#[test]
fn reads_a_custom_agent_as_its_base_and_names_settings_when_its_command_is_missing() {
    use natamux_agents::custom::set_custom_agents;
    use natamux_proto::agents::custom::CustomAgent;
    let agent = |id: &str, base: &str, command: &str| CustomAgent {
        id: id.into(),
        base: base.into(),
        command: command.into(),
    };
    set_custom_agents(vec![
        agent("claude-qwen", "claude-code", "cq"),
        agent("my-agent", "other", "my-agent"),
    ]);
    let f = Fixture::new();

    // A Claude Code variant has Claude Code's hooks: the bell is not a question.
    let qwen = f.running_task(Some("claude-qwen"));
    f.output(&qwen.id, "1", "\x07");
    assert_eq!(f.task(&qwen.id).state, TaskState::Running);
    // Another CLI has none: it is.
    let other = f.running_task(Some("my-agent"));
    f.output(&other.id, "1", "\x07");
    assert_eq!(f.task(&other.id).state, TaskState::NeedsInput);

    let task = f.running_task(Some("claude-qwen"));
    f.signals.watch_launch(&AgentLaunch {
        task_id: task.id.clone(),
        terminal: "1".to_string(),
        agent: "claude-qwen".to_string(),
        program: "cq".to_string(),
        session: None,
        restored: false,
    });
    f.output(
        &task.id,
        "1",
        "$ . '/data/agent-launch.sh'\r\nzsh: command not found: cq\r\n$ ",
    );
    let failed = f.tasks.list_events(&task.id).unwrap().pop().unwrap().event;
    let reason = failed.payload["reason"].as_str().unwrap();
    assert!(
        reason.contains("Install cq, or change its command in Settings > Agents"),
        "{reason}"
    );
}
