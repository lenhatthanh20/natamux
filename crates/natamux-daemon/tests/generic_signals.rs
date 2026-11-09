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
use natamux_proto::stages::catalog::full_pipeline;
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
        // POSIX: a launch echoes nothing. Windows cases use `windows_signals`.
        let signals =
            GenericSignals::with_platform(tasks.clone(), Some(Fixture::clock(&time)), false);
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
    let pipeline = full_pipeline();
    let settings: natamux_daemon::tasks::store::SettingsOf = Arc::new(move |_| pipeline.clone());
    let staged = Fixture::service(&f.dir, &f.time, Some(settings));
    let input = json!({ "title": "Export", "agent": "cursor" });
    let task = staged.create_task(Some(&input), &[], true).unwrap();
    let running = json!({
        "source": "human", "type": "state.overridden",
        "payload": { "to": "running", "stage": "build" },
    });
    staged.apply_event(&task.id, Some(&running)).unwrap();

    let signals =
        GenericSignals::with_platform(staged.clone(), Some(Fixture::clock(&f.time)), false);
    signals.on_output(&task.id, "1", b"\x07");
    signals.settle();

    let lanes = Value::Object(staged.get_task(&task.id).unwrap().lanes);
    assert_eq!(
        lanes,
        json!({ "test-spec": "running", "implement": "needs-input" })
    );
}

fn launch(f: &Fixture, task_id: &str) {
    launch_as(f, task_id, "claude-code", "claude", false);
}

/// A launch a second after the task started, as a stage starts its agent after its own events.
fn launch_as(f: &Fixture, task_id: &str, agent: &str, program: &str, restored: bool) {
    f.later(1);
    let launch = launch_of(task_id, agent, program, restored);
    f.signals.watch_launch(&launch);
}

fn launch_of(task_id: &str, agent: &str, program: &str, restored: bool) -> AgentLaunch {
    AgentLaunch {
        task_id: task_id.to_string(),
        terminal: "1".to_string(),
        agent: agent.to_string(),
        program: program.to_string(),
        session: None,
        restored,
    }
}

/// Signals for the launch lines of Windows, which echo the whole typed line.
fn windows_signals(f: &Fixture) -> GenericSignals {
    GenericSignals::with_platform(f.tasks.clone(), Some(Fixture::clock(&f.time)), true)
}

fn output_to(signals: &GenericSignals, id: &str, data: &str) {
    signals.on_output(id, "1", data.as_bytes());
    signals.settle();
}

/// What the shell echoes of the typed launch line.
const ECHO: &str = "$ . '/data/agent-launch.sh'\r\n";
const FAILED_MARK: &str = "\x1b]2;natamux-agent-failed\x07";

fn last_event(f: &Fixture, id: &str) -> natamux_proto::events::task_event::TaskEvent {
    f.tasks.list_events(id).unwrap().pop().unwrap().event
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
    f.later(31);
    f.output(&task.id, "1", "bash: claude: command not found\r\n");

    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn still_fails_a_missing_agent_seen_after_15_seconds() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    launch(&f, &task.id);
    f.later(20);
    f.output(&task.id, "1", "zsh: command not found: claude\r\n");

    assert_eq!(f.task(&task.id).state, TaskState::Failed);
}

#[test]
fn moves_the_task_to_needs_input_when_the_agent_asks_to_sign_in() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    launch(&f, &task.id);

    f.output(
        &task.id,
        "1",
        &format!("{ECHO}\x1b[31mNot logged in · Please"),
    );
    f.output(&task.id, "1", " run /login\x1b[0m\r\n");

    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    let asked = last_event(&f, &task.id);
    assert_eq!(asked.kind, "agent.asked");
    assert_eq!(asked.payload["reason"], "sign in");
    assert_eq!(asked.payload["terminal"], "1");
    let message = asked.payload["message"].as_str().unwrap();
    assert!(
        message.contains("\"Not logged in · Please run /login\""),
        "{message}"
    );
    assert!(message.contains("Sign in in Terminal 1"), "{message}");
}

#[test]
fn knows_the_sign_in_screen_of_each_built_in_agent() {
    // Raw bytes of Claude Code 2.1.319's first start and Codex 0.167.0: words put apart by moves.
    for (agent, program, line) in [
        (
            "claude-code",
            "claude",
            "\x1b[2G\x1b[1mChoose\x1b[9Gthe\x1b[13Gtext\x1b[18Gstyle\x1b[22m",
        ),
        (
            "codex",
            "codex",
            "\x1b[5;1H\x1b[2m> 1. \x1b[22mSign\x1b[5;11Hin with ChatGPT",
        ),
        ("codex", "codex", "Sign in with ChatGPT"),
        (
            "gemini",
            "gemini",
            "How would you like to authenticate for this project?",
        ),
        (
            "copilot",
            "copilot",
            "Please use /login to sign in to use Copilot",
        ),
        ("cursor", "cursor-agent", "Error: Authentication required."),
    ] {
        let f = Fixture::new();
        let task = f.running_task(Some(agent));
        launch_as(&f, &task.id, agent, program, false);
        f.output(&task.id, "1", &format!("{ECHO}{line}\r\n"));
        assert_eq!(f.task(&task.id).state, TaskState::NeedsInput, "{agent}");
    }
}

#[test]
fn pays_no_heed_to_a_sign_in_line_of_another_agent_later_or_a_second_time() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    launch(&f, &task.id);
    f.output(&task.id, "1", &format!("{ECHO}Sign in with ChatGPT\r\n"));
    f.output(&task.id, "2", "Please run /login\r\n");
    assert_eq!(f.task(&task.id).state, TaskState::Running);

    f.later(31);
    f.output(&task.id, "1", "Please run /login\r\n");
    assert_eq!(f.task(&task.id).state, TaskState::Running);

    let again = f.running_task(Some("claude-code"));
    launch(&f, &again.id);
    f.output(&again.id, "1", &format!("{ECHO}Please run /login\r\n"));
    f.apply(
        &again.id,
        json!({ "source": "agent", "type": "agent.resumed" }),
    );
    f.output(&again.id, "1", "Please run /login\r\n");
    assert_eq!(f.task(&again.id).state, TaskState::Running);
}

#[test]
fn reads_no_sign_in_line_in_the_windows_echo_of_the_typed_line_once() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    let signals = windows_signals(&f);
    f.later(1);
    signals.watch_launch(&launch_of(&task.id, "claude-code", "claude", false));
    let typed = "$Host.UI.RawUI.WindowTitle = 'natamux-agent'; claude 'Fix Please run /login'";

    output_to(&signals, &task.id, &typed[..40]);
    output_to(
        &signals,
        &task.id,
        &format!("{}\r\nWelcome back\r\n", &typed[40..]),
    );
    assert_eq!(f.task(&task.id).state, TaskState::Running);

    output_to(&signals, &task.id, "Please run /login\r\n");
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
}

#[test]
fn reads_the_first_posix_output_and_text_past_the_kept_tail() {
    let f = Fixture::new();
    let first = f.running_task(Some("claude-code"));
    launch(&f, &first.id);
    f.output(&first.id, "1", "Please run /login\r\n");
    assert_eq!(f.task(&first.id).state, TaskState::NeedsInput);

    let ended = f.running_task(Some("cursor"));
    launch_as(&f, &ended.id, "cursor", "cursor-agent", false);
    f.output(&ended.id, "1", &format!("Error: boom\r\n{FAILED_MARK}"));
    let reason = last_event(&f, &ended.id).payload["reason"].clone();
    assert!(
        reason.as_str().unwrap().contains(": \"Error: boom\""),
        "{reason}"
    );

    // A full-screen agent moves the cursor in place of new lines.
    let long = f.running_task(Some("claude-code"));
    launch(&f, &long.id);
    f.output(&long.id, "1", ECHO);
    f.output(&long.id, "1", &"x".repeat(5000));
    f.output(&long.id, "1", "\x1b[5;1HPlease run /login");
    assert_eq!(f.task(&long.id).state, TaskState::NeedsInput);
}

#[test]
fn fails_the_task_when_the_agent_ends_with_an_error_right_after_its_start() {
    let f = Fixture::new();
    let task = f.running_task(Some("cursor"));
    launch_as(&f, &task.id, "cursor", "cursor-agent", false);

    f.output(
        &task.id,
        "1",
        &format!("{ECHO}\x1b[31mError: not authenticated\x1b[0m\r\n{FAILED_MARK}$ "),
    );

    assert_eq!(f.task(&task.id).state, TaskState::Failed);
    let failed = last_event(&f, &task.id);
    assert_eq!(failed.kind, "agent.failed");
    assert_eq!(failed.payload["terminal"], "1");
    let reason = failed.payload["reason"].as_str().unwrap();
    assert!(
        reason.contains(": \"Error: not authenticated\""),
        "{reason}"
    );
    assert!(reason.contains("Read Terminal 1"), "{reason}");
    assert!(reason.contains("NATAMUX_CURSOR_ARGS"), "{reason}");
}

#[test]
fn quotes_nothing_when_the_ended_agent_printed_nothing() {
    let f = Fixture::new();
    let task = f.running_task(Some("codex"));
    launch_as(&f, &task.id, "codex", "codex", false);

    f.output(&task.id, "1", FAILED_MARK);

    let ended = last_event(&f, &task.id);
    let reason = ended.payload["reason"].as_str().unwrap();
    assert!(
        reason.starts_with("The agent ended right after it started. Read"),
        "{reason}"
    );
}

#[test]
fn ends_the_watch_at_the_sign_in_screen() {
    let f = Fixture::new();
    let task = f.running_task(Some("cursor"));
    launch_as(&f, &task.id, "cursor", "cursor-agent", false);
    f.output(
        &task.id,
        "1",
        &format!("{ECHO}Error: Authentication required.\r\n"),
    );
    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);

    f.output(&task.id, "1", FAILED_MARK);

    assert_eq!(f.task(&task.id).state, TaskState::NeedsInput);
    assert_eq!(last_event(&f, &task.id).kind, "agent.asked");
}

// Cursor's `-p` error and the failed mark: in one chunk or two, the task needs input.
#[test]
fn takes_a_sign_in_line_before_the_failed_mark_in_any_chunks() {
    let error = "Error: Authentication required.\r\n";
    for chunks in [
        vec![format!("{error}{FAILED_MARK}")],
        vec![error.to_string(), FAILED_MARK.to_string()],
    ] {
        let f = Fixture::new();
        let task = f.running_task(Some("cursor"));
        launch_as(&f, &task.id, "cursor", "cursor-agent", false);
        for chunk in &chunks {
            f.output(&task.id, "1", chunk);
        }
        assert_eq!(f.task(&task.id).state, TaskState::NeedsInput, "{chunks:?}");
        assert_eq!(last_event(&f, &task.id).kind, "agent.asked", "{chunks:?}");
    }
}

#[test]
fn counts_an_agent_event_at_the_time_of_the_launch() {
    let f = Fixture::new();
    let task = f.running_task(Some("claude-code"));
    launch(&f, &task.id);
    f.apply(
        &task.id,
        json!({ "source": "agent", "type": "agent.resumed" }),
    );

    f.output(&task.id, "1", &format!("{ECHO}{FAILED_MARK}"));

    assert_eq!(f.task(&task.id).state, TaskState::Running);
}

#[test]
fn takes_an_ignored_failed_mark_for_no_bell() {
    let f = Fixture::new();
    let late = f.running_task(Some("cursor"));
    launch_as(&f, &late.id, "cursor", "cursor-agent", false);
    f.later(31);
    f.output(&late.id, "1", "\x1b]2;natamux-agent-");
    f.output(&late.id, "1", "failed\x07");
    assert_eq!(f.task(&late.id).state, TaskState::Running);

    let reported = f.running_task(Some("copilot"));
    launch_as(&f, &reported.id, "copilot", "copilot", false);
    f.later(16);
    f.apply(
        &reported.id,
        json!({ "source": "agent", "type": "agent.resumed" }),
    );
    f.output(&reported.id, "1", FAILED_MARK);
    assert_eq!(f.task(&reported.id).state, TaskState::Running);

    // A real bell after the title still counts.
    f.later(11);
    f.output(&reported.id, "1", "Allow? \x07");
    assert_eq!(f.task(&reported.id).state, TaskState::NeedsInput);
}

#[test]
fn leaves_an_agent_that_reported_ended_late_or_was_restored() {
    let f = Fixture::new();
    let reported = f.running_task(Some("claude-code"));
    launch(&f, &reported.id);
    f.later(1);
    f.apply(
        &reported.id,
        json!({ "source": "agent", "type": "agent.finished" }),
    );
    f.apply(
        &reported.id,
        json!({ "source": "agent", "type": "agent.resumed" }),
    );
    f.output(&reported.id, "1", &format!("{ECHO}{FAILED_MARK}"));
    assert_eq!(f.task(&reported.id).state, TaskState::Running);

    let late = f.running_task(Some("claude-code"));
    launch(&f, &late.id);
    f.later(31);
    f.output(&late.id, "1", &format!("{ECHO}{FAILED_MARK}"));
    assert_eq!(f.task(&late.id).state, TaskState::Running);

    let restored = f.running_task(Some("claude-code"));
    launch_as(&f, &restored.id, "claude-code", "claude", true);
    f.output(
        &restored.id,
        "1",
        &format!("{ECHO}No conversation found\r\n{FAILED_MARK}"),
    );
    assert_eq!(f.task(&restored.id).state, TaskState::Running);
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

    // A variant shows the sign-in screen of its base; another CLI has none Natamux knows.
    let signing = f.running_task(Some("claude-qwen"));
    launch_as(&f, &signing.id, "claude-qwen", "cq", false);
    f.output(&signing.id, "1", &format!("{ECHO}Please run /login\r\n"));
    assert_eq!(f.task(&signing.id).state, TaskState::NeedsInput);
    let mine = f.running_task(Some("my-agent"));
    launch_as(&f, &mine.id, "my-agent", "my-agent", false);
    f.output(&mine.id, "1", &format!("{ECHO}Please run /login\r\n"));
    assert_eq!(f.task(&mine.id).state, TaskState::Running);
}
