//! A side-by-side group inside a branch path: the bug path runs Reproduce and Investigate at the
//! same time, then Fix.

use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState::{Done, Ready, Review, Running};
use serde_json::json;

use super::*;
use crate::events::derive::derive_task_state;
use crate::events::test_fixture::{applied, event, event_with, lanes, on, paths};
use crate::stages::position::Paths;

fn steps() -> Vec<Step> {
    serde_json::from_value(json!([
        {"id": "spec", "kind": "agent", "gate": "manual"},
        {"id": "kind", "paths": [
            {"id": "bug", "when": {"type": "command", "run": "true"}, "stages": [
                {"id": "triage", "parallel": [
                    {"id": "repro", "kind": "agent", "gate": "auto"},
                    {"id": "look", "kind": "agent", "gate": "manual", "output": "look.md"}
                ]},
                {"id": "fix", "kind": "agent", "gate": "auto"}
            ]},
            {"id": "other", "stages": [{"id": "plan", "kind": "agent", "gate": "manual"}]}
        ]},
        {"id": "release", "kind": "release", "gate": "manual"}
    ]))
    .unwrap()
}

fn bug() -> Paths {
    paths(&[("kind", "bug")])
}

/// The task in the group `triage`, with the state of each lane.
fn triage(repro: TaskState, look: TaskState) -> StagePosition {
    let lanes = lanes(&[("repro", repro), ("look", look)]);
    StagePosition {
        state: combined_state(&lanes),
        lanes,
        ..on("triage", Ready, bug())
    }
}

fn apply(position: &StagePosition, kind: &str, payload: serde_json::Value) -> AppliedEvent {
    apply_task_event(position, &event_with(kind, payload), &steps())
}

#[test]
fn enters_every_lane_of_a_group_that_starts_the_path() {
    let approve = json!({"stage": "spec", "path": "bug", "because": "true succeeds"});
    let result = apply(&on("spec", Review, vec![]), "stage.approved", approve);
    assert_eq!(result, applied(triage(Ready, Ready), true, true));
}

#[test]
fn waits_for_every_lane_then_goes_on_through_the_path_and_past_the_branch() {
    let one = apply(
        &triage(Running, Running),
        "stage.passed",
        json!({"stage": "repro"}),
    );
    assert_eq!(one, applied(triage(Done, Running), true, false));
    assert_eq!(one.position.state, Running);

    // The manual lane waits for a person; its approval ends the group.
    let checked = apply(&one.position, "stage.checked", json!({"stage": "look"}));
    assert_eq!(checked.position, triage(Done, Review));
    let both = apply(
        &checked.position,
        "stage.approved",
        json!({"stage": "look"}),
    );
    assert_eq!(both, applied(on("fix", Ready, bug()), true, true));

    let fixed = apply(&both.position, "stage.passed", json!({"stage": "fix"}));
    assert_eq!(fixed, applied(on("release", Ready, bug()), true, true));
}

#[test]
fn skips_a_lane_of_a_group_in_a_path_like_any_lane() {
    let skipped = apply(
        &triage(Running, Ready),
        "stage.skipped",
        json!({"stage": "look", "planned": true}),
    );
    assert_eq!(skipped.position, triage(Running, Done));
}

#[test]
fn sends_back_to_one_lane_and_keeps_the_path() {
    let from_fix = apply(
        &on("fix", Running, bug()),
        "stage.reopened",
        json!({"to": "repro"}),
    );
    assert_eq!(from_fix, applied(triage(Ready, Done), true, true));

    // Inside the group the other lane keeps its state.
    let inside = apply(
        &triage(Done, Running),
        "stage.reopened",
        json!({"to": "repro"}),
    );
    assert_eq!(inside.position, triage(Ready, Running));

    // Fix cannot send back to a later stage, and not to a path the task did not take.
    let later = apply(
        &triage(Running, Running),
        "stage.reopened",
        json!({"to": "fix"}),
    );
    assert!(!later.accepted);
    let other = apply(
        &on("fix", Running, bug()),
        "stage.reopened",
        json!({"to": "plan"}),
    );
    assert!(!other.accepted);
}

#[test]
fn sends_back_without_a_target_to_the_code_stage_of_the_group_before() {
    let asked = apply(
        &on("fix", Review, bug()),
        "stage.reopened",
        json!({"reason": "again"}),
    );
    assert_eq!(asked.position, triage(Ready, Done));
}

#[test]
fn gives_the_same_position_when_its_events_are_replayed() {
    let events = [
        event("task.readied"),
        event_with("stage.started", json!({"stage": "spec"})),
        event_with("stage.checked", json!({"stage": "spec"})),
        event_with("stage.approved", json!({"stage": "spec", "path": "bug"})),
        event_with("stage.started", json!({"stage": "repro"})),
        event_with("stage.started", json!({"stage": "look"})),
        event_with("stage.passed", json!({"stage": "repro"})),
        event_with("stage.passed", json!({"stage": "look"})),
    ];
    let (position, rejected) = derive_task_state(&events, &steps());
    assert!(rejected.is_empty());
    assert_eq!(position, on("fix", Ready, bug()));
}
