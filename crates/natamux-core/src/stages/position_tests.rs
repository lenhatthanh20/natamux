//! Ports the cases of `packages/core/src/stages/__tests__/position.test.ts` for the functions that
//! are in this crate. The others are tested in `natamux-proto`.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::TaskState::{Done, Draft, NeedsInput, Ready, Review, Running};

use super::*;
use crate::events::test_fixture::{at, at_lanes, branch, lanes, paths, plan, steps};

fn review_stage() -> Step {
    Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual))
}

#[test]
fn starts_a_task_in_draft_at_its_first_step() {
    assert_eq!(first_position(&default_pipeline()), at("context", Draft));
}

#[test]
fn starts_a_review_stage_waiting_for_you_and_a_group_with_a_state_per_stage() {
    let build = default_pipeline().remove(3);
    assert_eq!(
        enter_step(&review_stage(), None, None),
        at("review", Review)
    );
    assert_eq!(
        enter_step(&build, None, None),
        at_lanes(
            "build",
            Ready,
            &[("test-spec", Ready), ("implement", Ready)]
        )
    );
}

#[test]
fn shows_the_most_urgent_lane_of_a_group() {
    let combined = |list: &[(&str, TaskState)]| combined_state(&lanes(list));
    assert_eq!(combined(&[("a", Running), ("b", Review)]), Review);
    assert_eq!(
        combined(&[("a", NeedsInput), ("b", TaskState::Failed)]),
        NeedsInput
    );
    assert_eq!(combined(&[("a", Done), ("b", Running)]), Running);
    assert_eq!(combined(&[("a", Done), ("b", Done)]), Done);
}

#[test]
fn is_entered_at_the_first_stage_of_the_path_taken() {
    let branch = Step::Branch(branch());
    assert_eq!(
        enter_step(&branch, None, Some("pr-review")),
        at("review", Review)
    );
    assert_eq!(
        enter_step(&branch, Some(Draft), Some("small")),
        at("implement", Draft)
    );
}

#[test]
fn tells_the_stages_of_a_path_the_task_did_not_take() {
    let steps = steps();
    let small = paths(&[("after-plan", "small")]);
    let skipped = paths(&[("after-plan", "")]);
    assert!(is_not_taken(&steps, Some(&small), "review"));
    assert!(!is_not_taken(&steps, Some(&small), "implement"));
    assert!(is_not_taken(&steps, Some(&skipped), "implement"));
    assert!(!is_not_taken(&steps, Some(&small), "plan"));
    assert!(!is_not_taken(&steps, Some(&Vec::new()), "review"));
    assert!(!is_not_taken(&steps, None, "review"));
}

// Not in position.test.ts: `takenStages` and `entryState` have no case of their own there.
#[test]
fn takes_the_stages_of_the_path_a_task_took() {
    let ids = |step: &Step, taken: Option<&Paths>| -> Vec<String> {
        taken_stages(step, taken)
            .into_iter()
            .map(|stage| stage.id)
            .collect()
    };
    let branch = Step::Branch(branch());
    assert_eq!(
        ids(&branch, Some(&paths(&[("after-plan", "small")]))),
        ["implement", "testing"]
    );
    assert!(ids(&branch, Some(&paths(&[("after-plan", "")]))).is_empty());
    assert!(ids(&branch, Some(&Vec::new())).is_empty());
    assert_eq!(ids(&branch, None), ["implement", "testing", "review"]);
    assert_eq!(ids(&Step::Stage(plan()), None), ["plan"]);
    assert_eq!(entry_state(&plan()), Ready);
}
