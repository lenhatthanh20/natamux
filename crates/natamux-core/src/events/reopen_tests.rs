//! Ports `packages/core/src/events/__tests__/reopen.test.ts`.

use natamux_proto::tasks::task::TaskState::{Done, Ready, Review, Running};
use serde_json::json;

use crate::events::apply::complete_event;
use crate::events::test_fixture::{
    at, build, event, event_with, on, paths, run, small, steps, walk,
};

#[test]
fn sends_a_task_back_to_one_lane_of_a_group_and_keeps_the_other_result() {
    let reopened = event_with("stage.reopened", json!({"to": "implement"}));
    let (position, _) = run(at("review", Review), &[reopened]);
    assert_eq!(position, build(Ready, Done, Ready));
}

#[test]
fn sends_changes_asked_in_review_to_the_code_stage_of_the_step_before() {
    let (position, _) = run(at("review", Review), &[event("review.changes-requested")]);
    assert_eq!(position, build(Ready, Done, Ready));
}

#[test]
fn sends_changes_asked_about_a_stage_that_waits_for_approval_back_to_that_stage() {
    assert_eq!(
        run(at("spec", Review), &[event("review.changes-requested")]).0,
        at("spec", Ready)
    );
    let group = build(Review, Review, Running);
    assert_eq!(
        run(group, &[event("review.changes-requested")]).0,
        build(Running, Ready, Running)
    );
}

#[test]
fn keeps_the_other_stages_of_the_current_group_when_one_of_them_is_sent_back() {
    let group = build(Review, Review, Running);
    let reopened = event_with("stage.reopened", json!({"to": "implement"}));
    assert_eq!(run(group, &[reopened]).0, build(Review, Review, Ready));
}

#[test]
fn sends_a_task_back_to_a_stage_that_waits_for_a_person_in_its_waiting_state() {
    let reopened = event_with("stage.reopened", json!({"to": "review"}));
    assert_eq!(
        run(at("testing", Running), &[reopened]),
        (at("review", Review), vec![true])
    );
}

#[test]
fn does_not_send_a_task_forward_with_reopen() {
    let reopened = event_with("stage.reopened", json!({"to": "release"}));
    assert_eq!(run(at("spec", Review), &[reopened]).1, [false]);
}

#[test]
fn sends_a_task_back_only_to_a_stage_on_the_way_it_went() {
    let at_testing = on("testing", Running, small());
    let to_review = event_with("stage.reopened", json!({"to": "review"}));
    assert_eq!(
        walk(at_testing.clone(), &[to_review]),
        (at_testing.clone(), vec![false])
    );
    let to_implement = event_with("stage.reopened", json!({"to": "implement"}));
    assert_eq!(
        walk(at_testing, &[to_implement]).0,
        on("implement", Ready, small())
    );
    let at_implement = on("implement", Running, small());
    let to_testing = event_with("stage.reopened", json!({"to": "testing"}));
    assert_eq!(walk(at_implement, &[to_testing]).1, [false]);
    let skipped = on("release", Review, paths(&[("after-plan", "")]));
    let to_implement = event_with("stage.reopened", json!({"to": "implement"}));
    assert_eq!(walk(skipped, &[to_implement]).1, [false]);
}

#[test]
fn clears_the_choice_when_the_task_goes_back_to_a_stage_before_the_branch() {
    let reopened = event_with("stage.reopened", json!({"to": "plan"}));
    assert_eq!(
        walk(on("implement", Running, small()), &[reopened]).0,
        on("plan", Ready, vec![])
    );
    let overridden = event_with("state.overridden", json!({"to": "review", "stage": "plan"}));
    assert_eq!(
        walk(on("implement", Running, small()), &[overridden]).0,
        on("plan", Review, vec![])
    );
}

#[test]
fn sends_a_task_back_without_a_stage_named_along_the_path_it_took() {
    let back = |taken| walk(on("release", Review, taken), &[event("stage.reopened")]).0;
    assert_eq!(back(small()), on("implement", Ready, small()));
    assert_eq!(
        back(paths(&[("after-plan", "pr-review")])),
        on("plan", Ready, vec![])
    );
    assert_eq!(
        back(paths(&[("after-plan", "")])),
        on("plan", Ready, vec![])
    );
    let at_review = on("review", Review, paths(&[("after-plan", "pr-review")]));
    assert_eq!(
        walk(at_review, &[event("review.changes-requested")]).0,
        on("plan", Ready, vec![])
    );
    let completed = complete_event(
        &on("release", Review, small()),
        event("stage.reopened"),
        &steps(),
        &Vec::new(),
    );
    assert_eq!(
        serde_json::Value::Object(completed.payload),
        json!({"to": "implement"})
    );
}
