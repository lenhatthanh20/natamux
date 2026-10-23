use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::pipeline_rules::{IssueLevel, pipeline_issues};
use natamux_proto::stages::schema::{Gate, StageGroup, StageKind, Step, StepRef};

use super::*;
use crate::stages::editor_catalog::add_catalog_stage;
use crate::stages::editor_fields::set_enabled;
use crate::stages::editor_find::{find_stage, group_of, used_ids};
use crate::stages::editor_fixtures::{branch_ref, branched, group_ref, ids, sid, stage_step};

fn span(refs: Vec<StepRef>, from: usize, to: usize) -> Vec<StepRef> {
    refs[from..to].to_vec()
}

fn ref_ids(ids: &[&str]) -> Vec<StepRef> {
    ids.iter().map(|id| sid(id)).collect()
}

fn lanes(id: &str, stages: &[&str]) -> Step {
    Step::Group(StageGroup {
        id: id.to_string(),
        name: None,
        parallel: stages
            .iter()
            .map(|stage| {
                natamux_proto::stages::schema::Stage::new(stage, StageKind::Agent, Gate::Auto)
            })
            .collect(),
    })
}

#[test]
fn swaps_a_stage_with_the_step_above_or_below() {
    let steps = default_pipeline();
    assert_eq!(
        span(ids(&move_stage(&steps, "plan", -1)), 0, 3),
        ref_ids(&["context", "plan", "spec"])
    );
    assert_eq!(
        span(ids(&move_stage(&steps, "plan", 1)), 2, 4),
        vec![group_ref("build", &["test-spec", "implement"]), sid("plan")]
    );
}

#[test]
fn moves_a_stage_within_its_group_and_out_of_it_at_the_edge() {
    let steps = default_pipeline();
    assert_eq!(
        ids(&move_stage(&steps, "implement", -1))[3],
        group_ref("build", &["implement", "test-spec"])
    );
    // A group of one left runs on its own.
    assert_eq!(
        span(ids(&move_stage(&steps, "test-spec", -1)), 2, 5),
        ref_ids(&["plan", "test-spec", "implement"])
    );
    assert_eq!(
        span(ids(&move_stage(&steps, "implement", 1)), 3, 5),
        ref_ids(&["test-spec", "implement"])
    );
}

#[test]
fn does_nothing_at_the_ends() {
    let steps = default_pipeline();
    assert_eq!(move_stage(&steps, "context", -1), steps);
    assert!(!can_move(&steps, "context", -1));
    assert!(!can_move(&steps, "release", 1));
    assert!(can_move(&steps, "release", -1));
}

#[test]
fn puts_a_stage_before_or_after_another() {
    let steps = default_pipeline();
    assert_eq!(
        span(
            ids(&drop_stage(&steps, "testing", "spec", Side::Before)),
            0,
            3
        ),
        ref_ids(&["context", "testing", "spec"])
    );
    assert_eq!(
        span(
            ids(&drop_stage(&steps, "context", "plan", Side::After)),
            0,
            3
        ),
        ref_ids(&["spec", "plan", "context"])
    );
}

#[test]
fn reorders_the_stages_of_a_group_without_breaking_it() {
    let steps = drop_stage(&default_pipeline(), "implement", "test-spec", Side::Before);
    assert_eq!(
        ids(&steps)[3],
        group_ref("build", &["implement", "test-spec"])
    );
}

#[test]
fn lets_an_agent_stage_join_a_group_but_puts_a_review_next_to_it() {
    let steps = default_pipeline();
    assert_eq!(
        ids(&drop_stage(&steps, "plan", "implement", Side::After))[2],
        group_ref("build", &["test-spec", "implement", "plan"])
    );
    assert_eq!(
        span(
            ids(&drop_stage(&steps, "testing", "test-spec", Side::Before)),
            2,
            5
        ),
        vec![
            sid("plan"),
            sid("testing"),
            group_ref("build", &["test-spec", "implement"])
        ]
    );
}

#[test]
fn moves_a_whole_group() {
    let steps = default_pipeline();
    assert_eq!(
        span(
            ids(&drop_stage(&steps, "build", "spec", Side::Before)),
            0,
            3
        ),
        vec![
            sid("context"),
            group_ref("build", &["test-spec", "implement"]),
            sid("spec")
        ]
    );
}

#[test]
fn makes_a_new_group_of_a_stage_and_the_stage_above_with_an_unused_id() {
    let steps = join_above(&default_pipeline(), "plan");
    assert_eq!(ids(&steps)[1], group_ref("spec-group", &["spec", "plan"]));
    let added = add_catalog_stage(
        &add_catalog_stage(&steps, "research", None),
        "clarify",
        None,
    );
    let again = join_above(&added, "clarify");
    assert!(ids(&again).contains(&group_ref("research-group", &["research", "clarify"])));
}

#[test]
fn joins_the_group_above_up_to_4_stages() {
    let mut steps = drop_stage(&default_pipeline(), "plan", "testing", Side::Before);
    steps = join_above(&steps, "plan");
    assert_eq!(
        ids(&steps)[2],
        group_ref("build", &["test-spec", "implement", "plan"])
    );
    steps = add_catalog_stage(&steps, "docs", None);
    steps = drop_stage(&steps, "docs", "testing", Side::Before);
    steps = join_above(&steps, "docs");
    steps = add_catalog_stage(&steps, "refactor", None);
    steps = drop_stage(&steps, "refactor", "testing", Side::Before);
    assert_eq!(
        join_problem(&steps, "refactor").as_deref(),
        Some("A group holds 4 stages at most")
    );
}

#[test]
fn is_not_offered_for_review_testing_context_or_release_stages() {
    let steps = default_pipeline();
    let only = Some("Only agent and manual stages run side by side".to_string());
    assert_eq!(join_problem(&steps, "testing"), only);
    assert_eq!(join_problem(&steps, "release"), only);
    assert_eq!(
        join_problem(&steps, "spec").as_deref(),
        Some("Context cannot run side by side with other stages")
    );
    assert_eq!(join_problem(&steps, "context"), only);
    assert_eq!(
        join_problem(&steps, "implement").as_deref(),
        Some("It runs alongside other stages already")
    );
    assert_eq!(join_above(&steps, "testing"), steps);
}

#[test]
fn takes_a_stage_out_of_its_group_a_group_of_one_left_runs_on_its_own() {
    assert_eq!(
        span(ids(&run_alone(&default_pipeline(), "test-spec")), 3, 5),
        ref_ids(&["implement", "test-spec"])
    );
}

#[test]
fn c4_keeps_a_stage_off_when_it_moves() {
    let off = set_enabled(&default_pipeline(), "plan", false);
    let moved = move_stage(&off, "plan", -1);
    assert_eq!(
        span(ids(&moved), 0, 3),
        ref_ids(&["context", "plan", "spec"])
    );
    assert_eq!(
        find_stage(&moved, "plan").and_then(|stage| stage.enabled),
        Some(false)
    );
}

#[test]
fn c6_joins_the_stage_above_even_when_it_is_off() {
    let off = set_enabled(&default_pipeline(), "spec", false);
    let steps = join_above(&off, "plan");
    let group = group_of(&steps, "plan").map(|group| {
        group
            .parallel
            .iter()
            .map(|stage| stage.id.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(group, Some(vec!["spec".to_string(), "plan".to_string()]));
}

#[test]
fn c6_lets_a_manual_stage_run_alongside_an_agent_stage() {
    let steps = vec![
        stage_step("docs", StageKind::Agent, Gate::Auto),
        stage_step("qa", StageKind::Manual, Gate::Manual),
    ];
    assert_eq!(join_problem(&steps, "qa"), None);
    assert_eq!(
        ids(&join_above(&steps, "qa")),
        vec![group_ref("docs-group", &["docs", "qa"])]
    );
}

#[test]
fn c6_dissolves_a_group_of_two_when_one_of_its_stages_is_removed() {
    assert_eq!(
        span(ids(&remove_stage(&default_pipeline(), "implement")), 2, 5),
        ref_ids(&["plan", "test-spec", "testing"])
    );
}

#[test]
fn c6_offers_nothing_to_the_first_stage() {
    let steps = vec![stage_step("docs", StageKind::Agent, Gate::Auto)];
    assert_eq!(
        join_problem(&steps, "docs").as_deref(),
        Some("There is no stage above")
    );
}

#[test]
fn moves_a_stage_within_its_path() {
    let swapped = branch_ref(&["testing", "implement"], &["review"]);
    assert_eq!(ids(&move_stage(&branched(), "implement", 1))[2], swapped);
    assert_eq!(ids(&move_stage(&branched(), "testing", -1))[2], swapped);
}

#[test]
fn moves_a_stage_to_the_next_path_at_the_end_of_its_path_and_back() {
    assert_eq!(
        ids(&move_stage(&branched(), "testing", 1))[2],
        branch_ref(&["implement"], &["testing", "review"])
    );
    assert_eq!(
        ids(&move_stage(&branched(), "review", -1))[2],
        branch_ref(&["implement", "testing", "review"], &[])
    );
}

#[test]
fn moves_a_stage_out_of_the_branch_at_the_edge_of_the_first_or_last_path() {
    assert_eq!(
        span(ids(&move_stage(&branched(), "review", 1)), 2, 5),
        vec![
            branch_ref(&["implement", "testing"], &[]),
            sid("review"),
            sid("release")
        ]
    );
    assert_eq!(
        span(ids(&move_stage(&branched(), "implement", -1)), 1, 4),
        vec![
            sid("plan"),
            sid("implement"),
            branch_ref(&["testing"], &["review"])
        ]
    );
}

#[test]
fn moves_a_stage_next_to_a_branch_into_its_nearest_path() {
    let steps = move_stage(&branched(), "plan", 1);
    assert_eq!(
        ids(&steps),
        vec![
            sid("context"),
            branch_ref(&["plan", "implement", "testing"], &["review"]),
            sid("release")
        ]
    );
    let mut after = branched()[..3].to_vec();
    after.push(stage_step("docs", StageKind::Agent, Gate::Auto));
    after.extend(branched()[3..].iter().cloned());
    assert_eq!(
        ids(&move_stage(&after, "docs", -1))[2],
        branch_ref(&["implement", "testing"], &["review", "docs"])
    );
}

#[test]
fn moves_the_release_over_a_branch_and_the_branch_like_any_step() {
    assert_eq!(
        ids(&move_stage(&branched(), "release", -1))[2..].to_vec(),
        vec![
            sid("release"),
            branch_ref(&["implement", "testing"], &["review"])
        ]
    );
    assert_eq!(
        span(ids(&move_stage(&branched(), "after-plan", -1)), 1, 3),
        vec![
            branch_ref(&["implement", "testing"], &["review"]),
            sid("plan")
        ]
    );
}

#[test]
fn tells_when_a_stage_of_a_path_can_move() {
    assert!(can_move(&branched(), "implement", 1));
    assert!(can_move(&branched(), "review", 1));
    assert!(!can_move(&branched(), "release", 1));
}

#[test]
fn keeps_every_id_once_after_any_move() {
    for id in ["implement", "testing", "review", "plan"] {
        for direction in [-1, 1] {
            assert_eq!(
                used_ids(&move_stage(&branched(), id, direction)),
                used_ids(&branched())
            );
        }
    }
}

#[test]
fn drops_a_stage_before_or_after_a_stage_of_a_path() {
    let steps = branched();
    assert_eq!(
        ids(&drop_stage(&steps, "plan", "review", Side::Before))[1],
        branch_ref(&["implement", "testing"], &["plan", "review"])
    );
    assert_eq!(
        ids(&drop_stage(&steps, "testing", "implement", Side::Before))[2],
        branch_ref(&["testing", "implement"], &["review"])
    );
    assert_eq!(
        ids(&drop_stage(&steps, "implement", "review", Side::After))[2],
        branch_ref(&["testing"], &["review", "implement"])
    );
}

#[test]
fn drops_a_stage_on_a_path_to_its_end() {
    assert_eq!(
        ids(&drop_stage(&branched(), "plan", "pr-review", Side::After))[1],
        branch_ref(&["implement", "testing"], &["review", "plan"])
    );
    let empty = remove_stage(&branched(), "review");
    assert_eq!(
        ids(&drop_stage(&empty, "testing", "pr-review", Side::After))[2],
        branch_ref(&["implement"], &["testing"])
    );
}

#[test]
fn drops_a_stage_of_a_path_out_of_the_branch_next_to_a_step() {
    assert_eq!(
        span(
            ids(&drop_stage(
                &branched(),
                "implement",
                "release",
                Side::Before
            )),
            2,
            5
        ),
        vec![
            branch_ref(&["testing"], &["review"]),
            sid("implement"),
            sid("release")
        ]
    );
    assert_eq!(
        span(
            ids(&drop_stage(
                &branched(),
                "review",
                "after-plan",
                Side::After
            )),
            2,
            4
        ),
        vec![branch_ref(&["implement", "testing"], &[]), sid("review")]
    );
}

#[test]
fn does_not_drop_a_group_a_context_stage_or_a_release_stage_into_a_path() {
    let mut grouped = branched()[..3].to_vec();
    grouped.push(lanes("build", &["docs", "i18n"]));
    grouped.extend(branched()[3..].iter().cloned());
    assert_eq!(
        drop_stage(&grouped, "build", "review", Side::Before),
        grouped
    );
    let steps = branched();
    assert_eq!(drop_stage(&steps, "context", "review", Side::After), steps);
    assert_eq!(
        drop_stage(&steps, "release", "pr-review", Side::After),
        steps
    );
    assert_eq!(
        drop_stage(&steps, "after-plan", "review", Side::After),
        steps
    );
}

#[test]
fn removes_a_stage_of_a_path_and_leaves_the_path_with_none() {
    let steps = remove_stage(&branched(), "review");
    assert_eq!(ids(&steps)[2], branch_ref(&["implement", "testing"], &[]));
    let issues = pipeline_issues(&steps);
    assert!(
        issues
            .iter()
            .any(|issue| issue.stage.as_deref() == Some("after-plan")
                && issue.level == IssueLevel::Error)
    );
    assert_eq!(
        ids(&remove_stage(&branched(), "after-plan")),
        ref_ids(&["context", "plan", "release"])
    );
}

#[test]
fn does_not_run_a_stage_of_a_path_alongside_another() {
    let steps = branched();
    assert_eq!(
        join_problem(&steps, "testing").as_deref(),
        Some("A path holds single stages only")
    );
    assert_eq!(join_above(&steps, "testing"), steps);
    let mut after = steps[..3].to_vec();
    after.push(stage_step("docs", StageKind::Agent, Gate::Auto));
    after.extend(steps[3..].iter().cloned());
    assert_eq!(
        join_problem(&after, "docs").as_deref(),
        Some("After plan is a branch; a stage cannot run alongside it")
    );
    assert_eq!(join_above(&after, "docs"), after);
}
