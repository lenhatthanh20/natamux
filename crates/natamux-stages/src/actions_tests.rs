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

fn stored(accepted: bool, source: &str, kind: &str, payload: serde_json::Value) -> StoredEvent {
    serde_json::from_value(json!({
        "seq": 1, "accepted": accepted,
        "event": {"taskId": "T-1", "source": source, "type": kind, "at": "x", "payload": payload}
    }))
    .unwrap()
}

fn agent_back(from: &str) -> StoredEvent {
    let payload = json!({"to": "implement", "from": from, "byAgent": true});
    stored(true, "agent", "stage.reopened", payload)
}

#[test]
fn agent_send_backs_counts_the_agent_send_backs_from_one_stage() {
    let person = stored(true, "human", "stage.reopened", json!({"to": "implement"}));
    let events = [agent_back("review"), person, agent_back("review"), agent_back("testing")];
    assert_eq!(agent_send_backs(&events[2..], "review"), 1);
    assert_eq!(agent_send_backs(&events[2..], "testing"), 1);
    assert_eq!(agent_send_backs(&[], "review"), 0);
}

#[test]
fn agent_send_backs_start_again_after_a_person_steps_in() {
    let steps_in = [
        stored(true, "human", "stage.reopened", json!({"to": "implement"})),
        stored(true, "human", "review.changes-requested", json!({"reason": "x"})),
        stored(true, "human", "stage.approved", json!({"stage": "review"})),
        stored(true, "human", "review.approved", json!({})),
        stored(true, "human", "stage.passed", json!({"stage": "review"})),
        stored(true, "human", "stage.skipped", json!({"stage": "review"})),
        stored(true, "human", "state.overridden", json!({"to": "running"})),
    ];
    for person in steps_in {
        let kind = person.event.kind.clone();
        let events = [agent_back("review"), agent_back("review"), person, agent_back("review")];
        assert_eq!(agent_send_backs(&events, "review"), 1, "{kind}");
    }
}

#[test]
fn agent_send_backs_keep_counting_past_agent_events_and_refused_ones() {
    let no_reset = [
        stored(false, "human", "stage.reopened", json!({"to": "implement"})),
        stored(false, "human", "stage.approved", json!({"stage": "review"})),
        stored(true, "agent", "stage.reopened", json!({"to": "implement"})),
        stored(true, "agent", "stage.reported", json!({"stage": "implement"})),
        stored(true, "system", "stage.checked", json!({"stage": "implement"})),
        stored(true, "human", "task.edited", json!({})),
        agent_back("testing"),
    ];
    for other in no_reset {
        let kind = other.event.kind.clone();
        let events = [agent_back("review"), agent_back("review"), other];
        assert_eq!(agent_send_backs(&events, "review"), 2, "{kind}");
    }
}

#[test]
fn agent_send_backs_count_a_ci_send_back_by_the_system() {
    let payload = json!({"to": "implement", "from": "implement", "byAgent": true, "ci": true});
    let ci = stored(true, "system", "stage.reopened", payload);
    let person = stored(true, "human", "stage.reopened", json!({"to": "implement"}));
    assert_eq!(agent_send_backs(&[ci.clone(), ci.clone()], "implement"), 2);
    assert_eq!(agent_send_backs(&[ci.clone(), person, ci], "implement"), 1);
}
