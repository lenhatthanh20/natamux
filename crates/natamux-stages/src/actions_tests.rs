use natamux_proto::stages::schema::{Gate, StageGroup};
use serde_json::json;

use super::*;

fn task(state: TaskState, lanes: serde_json::Value) -> Task {
    let mut task: Task = serde_json::from_value(json!({"id": "T-1", "title": "x", "createdAt": "2026-01-01T00:00:00.000Z", "updatedAt": "2026-01-01T00:00:00.000Z"})).unwrap();
    task.state = state;
    task.lanes = lanes.as_object().cloned().unwrap_or_default();
    task
}

fn group() -> Step {
    Step::Group(StageGroup {
        id: "g".into(),
        name: None,
        parallel: vec![Stage::new("a", StageKind::Agent, Gate::Auto)],
    })
}

#[test]
fn lane_state_reads_the_lane_inside_a_group_and_the_task_state_outside() {
    let task = task(TaskState::Running, json!({"a": "review"}));
    assert_eq!(lane_state(&task, &group(), "a"), TaskState::Review);
    assert_eq!(lane_state(&task, &group(), "b"), TaskState::Done);
    let single = Step::Stage(Stage::new("a", StageKind::Agent, Gate::Auto));
    assert_eq!(lane_state(&task, &single, "a"), TaskState::Running);
}

#[test]
fn actions_parse_by_name() {
    for action in STAGE_ACTIONS {
        assert_eq!(StageAction::parse(action.as_str()), Some(action));
    }
    assert_eq!(StageAction::parse("nope"), None);
}

#[test]
fn run_by_agent_is_an_agent_stage_or_a_testing_stage_without_command() {
    let mut stage = Stage::new("t", StageKind::Testing, Gate::Auto);
    assert!(run_by_agent(&stage));
    stage.command = Some("make".into());
    assert!(!run_by_agent(&stage));
}

#[test]
fn send_back_reason_needs_a_reopen_with_a_reason() {
    let mut event: TaskEvent = serde_json::from_value(json!({
        "taskId": "T-1", "source": "human", "type": "stage.reopened", "at": "x",
        "payload": {"reason": "fix it"}
    }))
    .unwrap();
    assert_eq!(send_back_reason(Some(&event)), Some("fix it".into()));
    event.kind = "stage.approved".into();
    assert_eq!(send_back_reason(Some(&event)), None);
    assert_eq!(send_back_reason(None), None);
}
