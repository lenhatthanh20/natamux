//! Ports `packages/core/src/events/__tests__/apply.test.ts`.

use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState::{
    Cancelled, Done, Draft, Failed, NeedsInput, Ready, Review, Running,
};
use serde_json::{Value, json};

use super::*;
use crate::events::test_fixture::{
    applied, at, at_lanes, build, event, event_with, lanes, pipeline, run,
};

fn payload(event: TaskEvent) -> Value {
    Value::Object(event.payload)
}

fn no_pins() -> PinnedTerminals {
    Vec::new()
}

#[test]
fn accepts_record_only_events_in_any_state_without_a_change() {
    for kind in [
        "task.created",
        "git.pushed",
        "issue.linked",
        "context.fetched",
        "attachment.added",
        "stage.skip-set",
    ] {
        assert_eq!(
            run(at("plan", Running), &[event(kind)]),
            (at("plan", Running), vec![true])
        );
    }
}

#[test]
fn waits_for_approval_when_a_manual_stage_passes_its_checks_then_enters_the_next_step() {
    let (position, accepted) = run(
        at("spec", Ready),
        &[
            event("stage.started"),
            event("stage.reported"),
            event("stage.checked"),
        ],
    );
    assert_eq!(position, at("spec", Review));
    assert_eq!(accepted, [true, true, true]);

    let approved = apply_task_event(&position, &event("stage.approved"), &pipeline());
    assert_eq!(approved, applied(at("plan", Ready), true, true));
}

#[test]
fn enters_both_stages_of_a_group_as_ready() {
    let (position, _) = run(at("plan", Review), &[event("stage.approved")]);
    assert_eq!(position, build(Ready, Ready, Ready));
}

#[test]
fn needs_the_stage_name_when_an_event_fits_more_than_one_stage_of_a_group() {
    let group = build(Ready, Ready, Ready);
    assert_eq!(run(group.clone(), &[event("stage.started")]).1, [false]);
    let nope = event_with("stage.started", json!({"stage": "nope"}));
    assert_eq!(run(group, &[nope]).1, [false]);
}

#[test]
fn keeps_each_lane_on_its_own_and_shows_the_most_urgent_one() {
    let (position, _) = run(
        build(Ready, Ready, Ready),
        &[
            event_with("stage.started", json!({"stage": "test-spec"})),
            event_with("stage.started", json!({"stage": "implement"})),
            event_with("stage.checked", json!({"stage": "test-spec"})),
        ],
    );
    assert_eq!(position, build(Review, Review, Running));

    let asked = event_with("agent.asked", json!({"stage": "implement"}));
    let asking = run(position, &[asked]).0;
    assert_eq!(asking.state, NeedsInput);
}

#[test]
fn enters_the_next_step_only_when_every_lane_is_finished() {
    let group = build(Running, Review, Running);
    let checked = event_with("stage.checked", json!({"stage": "implement"}));
    let implemented = run(group, &[checked]).0;
    assert_eq!(implemented, build(Review, Review, Done));

    let approved = apply_task_event(
        &implemented,
        &event_with("stage.approved", json!({"stage": "test-spec"})),
        &pipeline(),
    );
    assert_eq!(approved, applied(at("review", Review), true, true));
}

#[test]
fn takes_a_failed_agent_stage_up_again_in_its_own_lane_then_moves_on() {
    let group = build(Failed, Done, Failed);
    let resumed_event = event_with("agent.resumed", json!({"stage": "implement"}));
    let resumed = run(group, &[resumed_event]).0;
    assert_eq!(resumed, build(Running, Done, Running));

    let checked = event_with("stage.checked", json!({"stage": "implement"}));
    let result = apply_task_event(&resumed, &checked, &pipeline());
    assert_eq!(result, applied(at("review", Review), true, true));
}

#[test]
fn takes_a_failed_check_to_needs_input_only_when_it_is_final() {
    let state = |event| run(at("plan", Running), &[event]).0.state;
    assert_eq!(state(event("stage.check-failed")), Running);
    assert_eq!(
        state(event_with("stage.check-failed", json!({"final": true}))),
        NeedsInput
    );
}

#[test]
fn lets_you_approve_a_stage_anyway_from_needs_input() {
    assert_eq!(
        run(at("plan", NeedsInput), &[event("stage.approved")]).0,
        build(Ready, Ready, Ready)
    );
}

#[test]
fn takes_an_event_with_no_stage_name_for_the_only_stage_of_a_group_it_fits() {
    let group = build(Review, Review, Running);
    assert_eq!(
        run(group.clone(), &[event("review.approved")]).0,
        build(Running, Done, Running)
    );
    assert_eq!(
        run(group, &[event("agent.asked")]).0.lanes,
        lanes(&[("test-spec", Review), ("implement", NeedsInput)])
    );
}

#[test]
fn takes_the_stage_of_the_terminal_an_event_came_from() {
    let group = build(Running, Running, Running);
    assert_eq!(run(group.clone(), &[event("agent.asked")]).1, [false]);
    let asked = event_with("agent.asked", json!({"terminal": "2"}));
    assert_eq!(
        run(group, &[asked]).0.lanes,
        lanes(&[("test-spec", NeedsInput), ("implement", Running)])
    );
}

#[test]
fn makes_a_stage_whose_agent_was_stopped_wait_to_start_again() {
    let stopped = |state| run(at("spec", state), &[event("agent.stopped")]);
    assert_eq!(stopped(Running).0, at("spec", Ready));
    assert_eq!(stopped(NeedsInput).0, at("spec", Ready));
    assert_eq!(stopped(Review).1, [false]);
    assert_eq!(stopped(Failed).1, [false]);
}

#[test]
fn stops_only_the_named_stage_of_a_group() {
    let group = build(NeedsInput, Review, NeedsInput);
    let stopped = event_with("agent.stopped", json!({"stage": "implement"}));
    assert_eq!(run(group, &[stopped]).0, build(Review, Review, Ready));
}

#[test]
fn skips_a_stage_without_output() {
    assert_eq!(
        run(at("context", Draft), &[event("stage.skipped")]).0,
        at("spec", Ready)
    );
}

#[test]
fn finishes_the_task_after_the_last_stage() {
    assert_eq!(
        run(at("release", Review), &[event("stage.approved")]).0,
        at("release", Done)
    );
}

#[test]
fn passes_an_auto_stage_when_its_checks_pass() {
    assert_eq!(
        run(at("testing", Running), &[event("stage.checked")]).0,
        at("release", Ready)
    );
}

#[test]
fn waits_for_you_when_the_agent_ends_its_turn_without_reporting_done() {
    assert_eq!(
        run(at("plan", Running), &[event("agent.finished")]).0,
        at("plan", NeedsInput)
    );
}

#[test]
fn keeps_a_stage_running_while_the_agent_waits_for_its_background_work() {
    assert_eq!(
        run(at("plan", Running), &[event("agent.waiting")]),
        (at("plan", Running), vec![true])
    );
    assert_eq!(
        run(at("plan", NeedsInput), &[event("agent.waiting")]).1,
        [false]
    );
}

#[test]
fn fails_a_stage_whose_command_failed() {
    assert_eq!(
        run(at("testing", Running), &[event("stage.failed")]).0,
        at("testing", Failed)
    );
}

#[test]
fn ignores_an_event_about_another_stage() {
    let checked = event_with("stage.checked", json!({"stage": "spec"}));
    assert_eq!(run(at("plan", Running), &[checked]).1, [false]);
}

#[test]
fn lets_a_human_override_set_any_state_and_move_to_a_stage() {
    let overridden = |position, payload| run(position, &[event_with("state.overridden", payload)]);
    assert_eq!(
        overridden(at("release", Cancelled), json!({"to": "ready"})).0,
        at("release", Ready)
    );
    assert_eq!(
        overridden(
            at("review", Review),
            json!({"to": "running", "stage": "plan"})
        )
        .0,
        at("plan", Running)
    );
    assert_eq!(
        overridden(at("plan", Ready), json!({"to": "nope"})).1,
        [false]
    );
}

#[test]
fn names_the_stage_an_event_is_about_when_the_sender_left_it_out() {
    let group = build(Review, Review, Running);
    let in_group = complete_event(&group, event("review.approved"), &pipeline(), &no_pins());
    assert_eq!(payload(in_group), json!({"stage": "test-spec"}));
    let alone = complete_event(
        &at("plan", Review),
        event("review.approved"),
        &pipeline(),
        &no_pins(),
    );
    assert_eq!(payload(alone), json!({"stage": "plan"}));
}

#[test]
fn names_the_stage_a_send_back_goes_to() {
    let asked = event_with("review.changes-requested", json!({"reason": "Fix it"}));
    let sent = complete_event(&at("review", Review), asked, &pipeline(), &no_pins());
    assert_eq!(
        payload(sent),
        json!({"reason": "Fix it", "to": "implement"})
    );
}

#[test]
fn leaves_an_event_that_names_its_stage_or_fits_no_stage_as_it_is() {
    let named = event_with("stage.checked", json!({"stage": "spec"}));
    let same = complete_event(&at("plan", Running), named.clone(), &pipeline(), &no_pins());
    assert_eq!(same, named);
    let group = build(Running, Running, Running);
    let unclear = event("agent.asked");
    assert_eq!(
        complete_event(&group, unclear.clone(), &pipeline(), &no_pins()),
        unclear
    );
}

// Test spec and Notes start their own agents, so they run in Terminals 7 and 8.
fn own_agent_steps() -> Vec<Step> {
    serde_json::from_value(json!([{
        "id": "build",
        "parallel": [
            {"id": "implement", "kind": "agent", "gate": "auto"},
            {"id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md", "agent": "copilot"},
            {"id": "notes", "kind": "agent", "gate": "auto", "output": "notes.md", "agent": "cursor"}
        ]
    }]))
    .unwrap()
}

fn own_agent_group() -> StagePosition {
    at_lanes(
        "build",
        Running,
        &[
            ("implement", Running),
            ("test-spec", Running),
            ("notes", Running),
        ],
    )
}

fn stage_from(terminal: &str, steps: &[Step]) -> Option<String> {
    let asked = event_with("agent.asked", json!({"terminal": terminal}));
    let done = complete_event(&own_agent_group(), asked, steps, &no_pins());
    done.payload
        .get("stage")
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[test]
fn names_the_stage_of_the_terminal_the_event_came_from() {
    let steps = own_agent_steps();
    assert_eq!(stage_from("1", &steps).as_deref(), Some("implement"));
    assert_eq!(stage_from("7", &steps).as_deref(), Some("test-spec"));
    assert_eq!(stage_from("8", &steps).as_deref(), Some("notes"));
}

#[test]
fn names_no_stage_for_a_terminal_no_stage_uses() {
    let steps = own_agent_steps();
    assert_eq!(stage_from("2", &steps), None);
    assert_eq!(stage_from("6", &steps), None);
}

#[test]
fn moves_only_the_lane_of_that_terminal() {
    let asked = event_with("agent.asked", json!({"terminal": "8"}));
    let result = apply_task_event(&own_agent_group(), &asked, &own_agent_steps());
    assert_eq!(
        result.position.lanes,
        lanes(&[
            ("implement", Running),
            ("test-spec", Running),
            ("notes", NeedsInput)
        ])
    );
}

// The settings then made Notes the stage that changes code, so they would give it Terminal 1. The
// terminals pinned at the start of the lanes still tell which stage rang the bell.
#[test]
fn names_the_stage_pinned_to_the_terminal_after_a_settings_change() {
    let changed: Vec<Step> = serde_json::from_value(json!([{
        "id": "build",
        "parallel": [
            {"id": "implement", "kind": "agent", "gate": "auto", "output": "impl.md"},
            {"id": "test-spec", "kind": "agent", "gate": "manual", "output": "test-spec.md"},
            {"id": "notes", "kind": "agent", "gate": "auto"}
        ]
    }]))
    .unwrap();
    let bell = event_with(
        "agent.asked",
        json!({"reason": "terminal bell", "terminal": "1"}),
    );
    let stage = |pinned: &PinnedTerminals| {
        let done = complete_event(&own_agent_group(), bell.clone(), &changed, pinned);
        done.payload
            .get("stage")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    assert_eq!(stage(&no_pins()).as_deref(), Some("notes"));
    let pinned = vec![
        ("implement".to_string(), "1".to_string()),
        ("test-spec".to_string(), "7".to_string()),
        ("notes".to_string(), "8".to_string()),
    ];
    assert_eq!(stage(&pinned).as_deref(), Some("implement"));
}
