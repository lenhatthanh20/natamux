//! Ports `packages/core/src/events/__tests__/derive.test.ts`.

use natamux_proto::stages::schema::{Gate, Stage, Step};
use natamux_proto::tasks::task::TaskState::{
    Cancelled, Done, Draft, Failed, Ready, Review, Running,
};
use serde_json::json;

use super::*;
use crate::events::apply::apply_task_event;
use crate::events::test_fixture::{
    applied, at, branch, event, event_with, legacy_pipeline, on, plan, small, steps,
};

#[test]
fn starts_in_draft_at_implement_with_no_events() {
    assert_eq!(
        derive_task_state(&[], &legacy_pipeline()),
        (at("implement", Draft), vec![])
    );
}

#[test]
fn waits_for_review_after_the_agent_reports_done_then_for_the_release_after_approval() {
    let result = derive_task_state(
        &[
            event("task.created"),
            event("task.readied"),
            event("agent.started"),
            event("agent.asked"),
            event("agent.resumed"),
            event("stage.reported"),
            event("stage.checked"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(result, (at("review", Review), vec![]));

    let approved = derive_task_state(
        &[
            event("task.readied"),
            event("agent.started"),
            event("stage.checked"),
            event("review.approved"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(approved.0, at("release", Ready));
}

#[test]
fn goes_back_to_implement_to_start_it_again_when_changes_are_requested() {
    let result = derive_task_state(
        &[
            event("task.readied"),
            event("agent.started"),
            event("stage.checked"),
            event("review.changes-requested"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(result.0, at("implement", Ready));
}

#[test]
fn moves_a_failed_stage_to_ready_on_retry() {
    let result = derive_task_state(
        &[
            event("task.readied"),
            event("agent.started"),
            event("agent.failed"),
            event("task.retried"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(result.0, at("implement", Ready));
}

#[test]
fn runs_a_failed_stage_again_when_its_agent_is_resumed() {
    let result = derive_task_state(
        &[
            event("task.readied"),
            event("agent.started"),
            event("agent.failed"),
            event("agent.resumed"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(result.0, at("implement", Running));
}

#[test]
fn fails_a_task_that_waited_for_the_user_when_its_agent_started_again_did_not_start() {
    let result = derive_task_state(
        &[
            event("task.readied"),
            event("agent.started"),
            event("agent.asked"),
            event("agent.failed"),
        ],
        &legacy_pipeline(),
    );
    assert_eq!(result.0, at("implement", Failed));
}

#[test]
fn skips_an_event_that_does_not_fit_and_reports_it() {
    let early = event("agent.finished");
    let result = derive_task_state(&[event("task.readied"), early.clone()], &legacy_pipeline());
    assert_eq!(result.0.state, Ready);
    assert_eq!(result.1, [early]);
}

#[test]
fn keeps_a_terminal_state_against_later_events() {
    let late = event("agent.started");
    let result = derive_task_state(&[event("task.cancelled"), late.clone()], &legacy_pipeline());
    assert_eq!(result.0.state, Cancelled);
    assert_eq!(result.1, [late]);
}

#[test]
fn finishes_a_task_in_review_or_release_when_its_pull_request_is_merged_and_only_then() {
    let steps = legacy_pipeline();
    assert_eq!(
        apply_task_event(&at("review", Review), &event("pr.merged"), &steps),
        applied(at("release", Done), true, true)
    );
    assert!(!apply_task_event(&at("implement", Running), &event("pr.merged"), &steps).accepted);
}

#[test]
fn gives_the_same_position_with_the_choices_when_its_log_is_replayed() {
    let mut log = vec![
        event("task.readied"),
        event_with("stage.started", json!({"stage": "plan"})),
        event_with("stage.checked", json!({"stage": "plan"})),
        event_with(
            "stage.approved",
            json!({"stage": "plan", "path": "small", "because": "plan.md has it"}),
        ),
        event_with("stage.started", json!({"stage": "implement"})),
        event_with("stage.checked", json!({"stage": "implement"})),
    ];
    assert_eq!(
        derive_task_state(&log, &steps()),
        (on("testing", Ready, small()), vec![])
    );
    log.extend([
        event_with("stage.reopened", json!({"to": "plan"})),
        event_with("stage.started", json!({"stage": "plan"})),
        event_with("stage.checked", json!({"stage": "plan"})),
        event_with("stage.approved", json!({"stage": "plan"})),
    ]);
    assert_eq!(
        derive_task_state(&log, &steps()),
        (
            on("release", Ready, vec![("after-plan".into(), String::new())]),
            vec![]
        )
    );
}

#[test]
fn tells_when_an_event_would_enter_a_branch_without_a_path() {
    let approve = event("stage.approved");
    let found = Some(branch());
    assert_eq!(
        enters_branch(&on("plan", Review, vec![]), &approve, &steps()),
        found
    );
    assert_eq!(
        enters_branch(&at("plan", Review), &approve, &steps()),
        found
    );
    let small_path = event_with("stage.approved", json!({"path": "small"}));
    assert_eq!(
        enters_branch(&on("plan", Review, vec![]), &small_path, &steps()),
        found
    );
    let checked = event("stage.checked");
    assert_eq!(
        enters_branch(&on("plan", Running, vec![]), &checked, &steps()),
        None
    );
    let mut auto = steps();
    auto[0] = Step::Stage(Stage {
        gate: Gate::Auto,
        ..plan()
    });
    assert_eq!(
        enters_branch(&on("plan", Running, vec![]), &checked, &auto),
        found
    );
    let at_implement = on("implement", Running, small());
    assert_eq!(enters_branch(&at_implement, &checked, &steps()), None);
    let at_testing = on("testing", Running, small());
    assert_eq!(enters_branch(&at_testing, &checked, &steps()), None);
    let reopened = event_with("stage.reopened", json!({"to": "plan"}));
    assert_eq!(enters_branch(&at_testing, &reopened, &steps()), None);
}
