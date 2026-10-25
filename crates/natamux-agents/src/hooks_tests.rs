use super::*;
use natamux_core::events::apply::apply_task_event;
use natamux_core::stages::position::StagePosition;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use serde_json::json;

const STATES: [TaskState; 9] = [
    TaskState::Draft,
    TaskState::Ready,
    TaskState::Running,
    TaskState::NeedsInput,
    TaskState::Review,
    TaskState::Blocked,
    TaskState::Failed,
    TaskState::Done,
    TaskState::Cancelled,
];

fn kinds(state: TaskState, signal: Signal) -> Vec<&'static str> {
    plan_signal_events(state, signal, &Map::new())
        .iter()
        .map(|planned| planned.kind)
        .collect()
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
    let cases: [(TaskState, Vec<&str>); 9] = [
        (TaskState::Draft, vec!["task.readied", "agent.started"]),
        (TaskState::Ready, vec!["agent.started"]),
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

#[test]
fn first_resumes_then_finishes_when_the_agent_was_waiting_for_the_user() {
    assert_eq!(
        kinds(TaskState::NeedsInput, Signal::TurnEnded),
        ["agent.resumed", "agent.finished"]
    );
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
fn first_resumes_then_waits_when_the_agent_was_waiting_for_the_user() {
    assert_eq!(
        kinds(TaskState::NeedsInput, Signal::TurnPaused),
        ["agent.resumed", "agent.waiting"]
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
        kinds(TaskState::NeedsInput, Signal::Activity),
        ["agent.resumed"]
    );
    assert!(kinds(TaskState::Running, Signal::Activity).is_empty());
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
    let planned = plan_signal_events(TaskState::Draft, Signal::Prompt, &payload);
    let payloads: Vec<_> = planned.iter().map(|event| &event.payload).collect();
    assert_eq!(payloads, [&payload, &payload]);
}

#[test]
fn only_plans_events_the_state_machine_accepts_in_every_state() {
    let steps = legacy_pipeline();
    for start in STATES {
        for signal in SIGNALS {
            let stage = if start == TaskState::Review {
                "review"
            } else {
                "implement"
            };
            let mut position = StagePosition::new(stage, start);
            for planned in plan_signal_events(start, signal, &Map::new()) {
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
