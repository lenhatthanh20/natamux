use super::*;
use natamux_core::events::apply::apply_task_event;
use natamux_core::stages::position::StagePosition;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use serde_json::json;

const STATES: [TaskState; 10] = [
    TaskState::Draft,
    TaskState::Ready,
    TaskState::Running,
    TaskState::Idle,
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Blocked,
    TaskState::Failed,
    TaskState::Done,
    TaskState::Cancelled,
];

fn kinds_in(state: TaskState, wait: Wait, signal: Signal) -> Vec<&'static str> {
    plan_signal_events(state, wait, signal, &Map::new())
        .iter()
        .map(|planned| planned.kind)
        .collect()
}

// A wait for an answer, as after `natamux ask` or a question in plain text.
fn kinds(state: TaskState, signal: Signal) -> Vec<&'static str> {
    kinds_in(state, Wait::Answer, signal)
}

// The pipeline of a task made before stages existed.
fn legacy_pipeline() -> Vec<Step> {
    vec![
        Step::Stage(Stage::new("implement", StageKind::Agent, Gate::Auto)),
        Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual)),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ]
}

#[test]
fn maps_a_prompt_in_a_task_that_is_each_state() {
    let cases: [(TaskState, Vec<&str>); 10] = [
        (TaskState::Draft, vec!["task.readied", "agent.started"]),
        (TaskState::Ready, vec!["agent.started"]),
        (TaskState::Idle, vec!["agent.resumed"]),
        (TaskState::NeedsInput, vec!["agent.resumed"]),
        (TaskState::Review, vec!["agent.resumed"]),
        (TaskState::Running, vec![]),
        (TaskState::Failed, vec!["agent.resumed"]),
        (TaskState::Blocked, vec![]),
        (TaskState::Done, vec![]),
        (TaskState::Cancelled, vec![]),
    ];
    for (state, expected) in cases {
        assert_eq!(kinds(state, Signal::Prompt), expected, "{state:?}");
    }
}

#[test]
fn treats_the_end_of_a_turn_as_finished_when_the_agent_was_running() {
    assert_eq!(
        kinds(TaskState::Running, Signal::TurnEnded),
        ["agent.finished"]
    );
}

// T-0008: `natamux ask`, then the turn ended. The question still waits for the user.
#[test]
fn keeps_a_task_that_waits_for_the_user_waiting_when_the_turn_ends() {
    for signal in [Signal::TurnEnded, Signal::TurnPaused, Signal::TurnAsked] {
        assert!(
            kinds(TaskState::NeedsInput, signal).is_empty(),
            "{}",
            signal.as_str()
        );
    }
}

// a permission the user denied leaves no tool hook; the turn end ends that wait.
#[test]
fn ends_a_hooks_wait_at_the_end_of_the_turn() {
    let hook = |signal| kinds_in(TaskState::NeedsInput, Wait::Hook, signal);
    assert_eq!(hook(Signal::TurnEnded), ["agent.resumed", "agent.finished"]);
    assert_eq!(hook(Signal::TurnPaused), ["agent.resumed", "agent.waiting"]);
    assert_eq!(hook(Signal::TurnAsked), ["agent.resumed", "agent.asked"]);
    assert!(hook(Signal::Asking).is_empty());
}

#[test]
fn first_resumes_then_finishes_when_the_agent_was_idle() {
    assert_eq!(
        kinds(TaskState::Idle, Signal::TurnEnded),
        ["agent.resumed", "agent.finished"]
    );
}

#[test]
fn a_turn_that_ends_on_a_question_makes_the_task_wait_for_the_user() {
    assert_eq!(
        kinds(TaskState::Running, Signal::TurnAsked),
        ["agent.asked"]
    );
    assert_eq!(kinds(TaskState::Idle, Signal::TurnAsked), ["agent.asked"]);
    assert!(kinds(TaskState::Review, Signal::TurnAsked).is_empty());
    assert!(kinds(TaskState::Done, Signal::TurnAsked).is_empty());
}

// The agent stopped without a question, so nothing waits for the user.
#[test]
fn an_ended_turn_makes_the_task_idle_and_a_question_makes_it_need_input() {
    let steps = legacy_pipeline();
    let apply = |state: TaskState, signal: Signal| {
        let mut position = StagePosition::new("implement", state);
        for planned in plan_signal_events(state, Wait::Answer, signal, &Map::new()) {
            let event = TaskEvent {
                task_id: "T-0001".into(),
                source: "agent".into(),
                kind: planned.kind.into(),
                at: "2026-10-08T09:00:00.000Z".into(),
                payload: Map::new(),
            };
            position = apply_task_event(&position, &event, &steps).position;
        }
        position.state
    };
    assert_eq!(
        apply(TaskState::Running, Signal::TurnEnded),
        TaskState::Idle
    );
    assert_eq!(
        apply(TaskState::Running, Signal::Asking),
        TaskState::NeedsInput
    );
    assert_eq!(
        apply(TaskState::Idle, Signal::Asking),
        TaskState::NeedsInput
    );
    assert_eq!(apply(TaskState::Idle, Signal::Prompt), TaskState::Running);
    assert_eq!(apply(TaskState::Idle, Signal::Activity), TaskState::Running);
}

#[test]
fn ignores_the_end_of_a_turn_in_a_task_that_is_not_running_or_waiting() {
    let ignored = [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ];
    for state in ignored {
        assert!(kinds(state, Signal::TurnEnded).is_empty(), "{state:?}");
    }
}

#[test]
fn keeps_a_running_task_running_when_the_agent_pauses_for_background_work() {
    assert_eq!(
        kinds(TaskState::Running, Signal::TurnPaused),
        ["agent.waiting"]
    );
}

#[test]
fn ignores_a_paused_turn_in_a_task_that_is_not_running_or_waiting() {
    let ignored = [
        TaskState::Draft,
        TaskState::Ready,
        TaskState::Review,
        TaskState::Blocked,
        TaskState::Failed,
        TaskState::Done,
        TaskState::Cancelled,
    ];
    for state in ignored {
        assert!(kinds(state, Signal::TurnPaused).is_empty(), "{state:?}");
    }
}

#[test]
fn maps_a_failure_to_failed() {
    assert_eq!(kinds(TaskState::Running, Signal::Failed), ["agent.failed"]);
    assert_eq!(
        kinds(TaskState::NeedsInput, Signal::Failed),
        ["agent.resumed", "agent.failed"]
    );
}

#[test]
fn does_not_fail_a_failed_task_twice() {
    assert!(kinds(TaskState::Failed, Signal::Failed).is_empty());
}

#[test]
fn maps_a_question_of_a_running_agent_to_needs_input_and_ignores_it_otherwise() {
    assert_eq!(kinds(TaskState::Running, Signal::Asking), ["agent.asked"]);
    assert!(kinds(TaskState::Ready, Signal::Asking).is_empty());
    assert!(kinds(TaskState::Review, Signal::Asking).is_empty());
}

#[test]
fn resumes_a_waiting_task_when_the_agent_works_and_ignores_the_call_otherwise() {
    assert_eq!(
        kinds_in(TaskState::NeedsInput, Wait::Hook, Signal::Activity),
        ["agent.resumed"]
    );
    assert!(kinds(TaskState::Running, Signal::Activity).is_empty());
}

// `natamux ask` returns at once; a tool the agent runs after it is no answer.
#[test]
fn keeps_waiting_for_an_answer_while_the_agent_works() {
    assert!(kinds(TaskState::NeedsInput, Signal::Activity).is_empty());
    assert_eq!(
        kinds(TaskState::NeedsInput, Signal::Prompt),
        ["agent.resumed"]
    );
}

// After an API error the session lives on, so a tool that runs shows the agent works again.
#[test]
fn resumes_a_failed_task_when_the_agent_works_again() {
    assert_eq!(
        kinds(TaskState::Failed, Signal::Activity),
        ["agent.resumed"]
    );
}

#[test]
fn passes_the_payload_to_every_event_it_plans() {
    let payload = json!({ "hook": "x" });
    let payload = payload.as_object().cloned().unwrap_or_default();
    let planned = plan_signal_events(TaskState::Draft, Wait::Answer, Signal::Prompt, &payload);
    let payloads: Vec<_> = planned.iter().map(|event| &event.payload).collect();
    assert_eq!(payloads, [&payload, &payload]);
}

#[test]
fn only_plans_events_the_state_machine_accepts_in_every_state() {
    let steps = legacy_pipeline();
    for (start, wait) in STATES
        .into_iter()
        .flat_map(|state| [(state, Wait::Hook), (state, Wait::Answer)])
    {
        for signal in SIGNALS {
            let stage = if start == TaskState::Review {
                "review"
            } else {
                "implement"
            };
            let mut position = StagePosition::new(stage, start);
            for planned in plan_signal_events(start, wait, signal, &Map::new()) {
                let event = TaskEvent {
                    task_id: "T-0001".into(),
                    source: "agent".into(),
                    kind: planned.kind.into(),
                    at: "2026-10-08T09:00:00.000Z".into(),
                    payload: Map::new(),
                };
                let applied = apply_task_event(&position, &event, &steps);
                assert!(applied.accepted, "{} in {start:?}", signal.as_str());
                position = applied.position;
            }
        }
    }
}

#[test]
fn finds_natamux_ask_in_a_shell_command() {
    let asks = [
        "natamux ask \"Which branch?\"",
        "cd a && natamux ask -",
        "/opt/bin/natamux ask x",
        "natamux ask --stage spec 'x'",
    ];
    for command in asks {
        assert!(runs_ask(command), "{command}");
    }
    let others = [
        "natamux done",
        "echo natamux asked",
        "natamuxask x",
        "ask natamux",
    ];
    for command in others {
        assert!(!runs_ask(command), "{command}");
    }
    // Accepted: it only skips one after-tool hook while the agent runs.
    assert!(runs_ask("grep \"natamux ask\" notes.md"));
}

#[test]
fn reads_a_question_from_the_last_line_of_a_message() {
    let asked = [
        ("Done.\n\nShould I merge it?", "Should I merge it?"),
        ("Pick one:\n1. A\n2. B\n\n**Which one?**\n", "Which one?"),
        ("Tiếp tục không？", "Tiếp tục không？"),
        ("  Use `main` (the default)?)", "Use `main` (the default)?"),
        (
            "Two ways:\n\nWhich one?\n1. Keep it\n2) Drop it\n- or merge\na) later",
            "Which one?",
        ),
    ];
    for (message, question) in asked {
        assert_eq!(question_of(message), Some(question), "{message}");
    }
    for message in [
        "Why? Because the test failed.\nFixed it.",
        "Done?\nOK. Merged.",
        "Changes:\n1. A\n2. B",
        "1. A\n2. B",
        "All tests pass.",
        "",
    ] {
        assert_eq!(question_of(message), None, "{message}");
    }
}

fn summary(tool: &str, input: Value) -> Option<String> {
    tool_summary(Some(tool), input.as_object(), Some("/w"))
}

#[test]
fn says_a_tool_call_as_a_line() {
    let cases = [
        (
            "Edit",
            json!({ "file_path": "/w/src/a.ts" }),
            "Edit src/a.ts",
        ),
        (
            "Read",
            json!({ "file_path": "/other/b.ts" }),
            "Read /other/b.ts",
        ),
        (
            "Bash",
            json!({ "command": "pnpm  test\n--run" }),
            "Bash: pnpm test --run",
        ),
        ("Grep", json!({ "pattern": "TODO" }), "Grep TODO"),
        ("TodoWrite", json!({}), "TodoWrite"),
    ];
    for (tool, input, text) in cases {
        assert_eq!(summary(tool, input).as_deref(), Some(text), "{tool}");
    }
}

#[test]
fn cuts_a_long_line() {
    let line = summary("Bash", json!({ "command": "x".repeat(200) }));
    assert_eq!(line.map(|line| utf16_len(&line)), Some(80));
}

#[test]
fn has_nothing_to_say_without_a_tool() {
    assert_eq!(tool_summary(None, None, None), None);
}

#[test]
fn shortens_in_utf16_units() {
    // Each emoji is two units: 39 of them and the ellipsis are 79 units, then one more is cut.
    let line = shorten(&"😀".repeat(50), 80);
    assert_eq!(line, format!("{}…", "😀".repeat(39)));
    assert_eq!(shorten("  a \t\n b  ", 80), "a b");
}
