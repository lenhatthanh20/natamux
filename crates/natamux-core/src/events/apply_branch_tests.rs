//! Ports the `a branch` cases of `packages/core/src/events/__tests__/apply.test.ts`.

use natamux_proto::stages::schema::{StageBranch, Step};
use natamux_proto::tasks::task::TaskState::{Done, Ready, Review, Running};
use serde_json::{Value, json};

use super::*;
use crate::events::test_fixture::{
    applied, at, branch, event, event_with, on, paths, plan, small, steps, walk,
};

fn payload(event: TaskEvent) -> Value {
    Value::Object(event.payload)
}

fn no_pins() -> PinnedTerminals {
    Vec::new()
}

#[test]
fn enters_the_first_stage_of_the_path_the_event_names_and_records_it() {
    let approve = event_with(
        "stage.approved",
        json!({"stage": "plan", "path": "small", "because": "plan.md has it"}),
    );
    let result = apply_task_event(&on("plan", Review, vec![]), &approve, &steps());
    assert_eq!(result, applied(on("implement", Ready, small()), true, true));
    let review = event_with("stage.approved", json!({"path": "pr-review"}));
    assert_eq!(
        walk(on("plan", Review, vec![]), &[review]).0,
        on("review", Review, paths(&[("after-plan", "pr-review")]))
    );
}

#[test]
fn skips_the_branch_when_the_event_names_no_path_or_a_path_the_task_does_not_have() {
    let skipped = on("release", Ready, paths(&[("after-plan", "")]));
    let result = apply_task_event(
        &on("plan", Review, vec![]),
        &event("stage.approved"),
        &steps(),
    );
    assert_eq!(result, applied(skipped.clone(), true, true));
    let gone = event_with("stage.approved", json!({"path": "gone"}));
    assert_eq!(walk(on("plan", Review, vec![]), &[gone]).0, skipped);
    // A position from before branches existed has no record of the choices yet.
    assert_eq!(
        walk(at("plan", Review), &[event("stage.approved")]).0,
        skipped
    );
}

#[test]
fn takes_the_path_of_an_event_only_at_the_first_branch_it_reaches() {
    let next: StageBranch = serde_json::from_value(json!({
        "id": "after-build",
        "paths": [
            {"id": "docs", "when": {"type": "command", "run": "true"},
             "stages": [{"id": "write-docs", "kind": "agent", "gate": "auto"}]},
            {"id": "none", "stages": [{"id": "qa", "kind": "manual", "gate": "manual"}]}
        ]
    }))
    .unwrap();
    let two = vec![
        Step::Stage(plan()),
        Step::Branch(branch()),
        Step::Branch(next),
    ];
    let approved = event_with("stage.approved", json!({"path": "docs"}));
    let result = apply_task_event(&on("plan", Review, vec![]), &approved, &two);
    assert_eq!(
        result.position,
        StagePosition {
            stage: "plan".into(),
            state: Done,
            lanes: vec![],
            paths: Some(paths(&[("after-plan", ""), ("after-build", "")])),
        }
    );
}

#[test]
fn goes_through_the_stages_of_the_path_then_to_the_step_after_the_branch() {
    let (position, accepted) = walk(
        on("implement", Running, small()),
        &[
            event("stage.checked"),
            event("stage.started"),
            event("agent.asked"),
            event("agent.resumed"),
            event("stage.checked"),
        ],
    );
    assert_eq!(accepted, [true; 5]);
    assert_eq!(position, on("release", Ready, small()));
    assert_eq!(
        walk(on("implement", Ready, small()), &[event("stage.skipped")]).0,
        on("testing", Ready, small())
    );
    let pr_review = paths(&[("after-plan", "pr-review")]);
    assert_eq!(
        walk(
            on("review", Review, pr_review.clone()),
            &[event("review.approved")]
        ),
        (on("release", Ready, pr_review), vec![true])
    );
}

#[test]
fn names_the_stage_in_a_path_an_event_is_about_and_keeps_the_path_and_the_reason() {
    let checked = complete_event(
        &on("implement", Running, small()),
        event("stage.checked"),
        &steps(),
        &no_pins(),
    );
    assert_eq!(payload(checked), json!({"stage": "implement"}));
    let chosen = event_with(
        "stage.approved",
        json!({"path": "small", "because": "Otherwise"}),
    );
    let done = complete_event(&on("plan", Review, vec![]), chosen, &steps(), &no_pins());
    assert_eq!(
        payload(done),
        json!({"path": "small", "because": "Otherwise", "stage": "plan"})
    );
}

#[test]
fn finishes_a_task_on_a_path_when_its_pull_request_is_merged() {
    let pr_review = paths(&[("after-plan", "pr-review")]);
    let at_review = on("review", Review, pr_review.clone());
    assert_eq!(
        apply_task_event(&at_review, &event("pr.merged"), &steps()),
        applied(on("release", Done, pr_review), true, true)
    );
}

#[test]
fn finishes_a_task_on_the_last_stage_it_took_when_the_pipeline_ends_in_a_branch() {
    let pr_review = paths(&[("after-plan", "pr-review")]);
    let at_review = on("review", Review, pr_review.clone());
    let ends_in_branch = vec![Step::Stage(plan()), Step::Branch(branch())];
    assert_eq!(
        apply_task_event(&at_review, &event("pr.merged"), &ends_in_branch),
        applied(on("review", Done, pr_review), true, false)
    );
}

#[test]
fn refuses_a_merge_at_a_stage_of_a_path_before_its_review_stage() {
    let reviewed: StageBranch = serde_json::from_value(json!({
        "id": "after-plan",
        "paths": [{"id": "full", "stages": [
            {"id": "implement", "kind": "agent", "gate": "auto"},
            {"id": "review", "kind": "review", "gate": "manual"}
        ]}]
    }))
    .unwrap();
    let full = vec![Step::Stage(plan()), Step::Branch(reviewed)];
    let merge = |stage: &str| {
        let position = on(stage, Running, paths(&[("after-plan", "full")]));
        apply_task_event(&position, &event("pr.merged"), &full)
    };
    assert!(!merge("implement").accepted);
    assert!(merge("review").accepted);
}
