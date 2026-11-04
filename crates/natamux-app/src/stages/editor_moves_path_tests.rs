//! Moves of stages and groups inside a path of a branch.

use natamux_proto::stages::schema::{Step, StepRef};

use super::*;
use crate::stages::editor_catalog::add_catalog_stage;
use crate::stages::editor_find::used_ids;
use crate::stages::editor_fixtures::{branch_refs, branched, group_ref, ids, path_grouped, sid};

/// The branch of `path_grouped` with these items in its Small change path.
fn small(items: Vec<StepRef>) -> StepRef {
    branch_refs(items, vec![sid("review")])
}

fn build(lanes: &[&str]) -> StepRef {
    group_ref("build", lanes)
}

/// A branch inside a path: the editor must never make one.
fn has_nested_branch(steps: &[Step]) -> bool {
    let paths = steps.iter().filter_map(|step| match step {
        Step::Branch(branch) => Some(&branch.paths),
        _ => None,
    });
    paths
        .flatten()
        .any(|path| path.stages.iter().any(Step::is_branch))
}

#[test]
fn makes_a_group_of_a_path_stage_and_the_stage_above_it_in_the_path() {
    let added = add_catalog_stage(&branched(), "docs", Some("small"));
    let steps = move_stage(&added, "docs", -1);
    assert_eq!(join_problem(&steps, "docs"), None);
    let joined = join_above(&steps, "docs");
    let group = group_ref("implement-group", &["implement", "docs"]);
    assert_eq!(ids(&joined)[2], small(vec![group, sid("testing")]));
}

#[test]
fn joins_a_group_of_a_path_up_to_4_stages() {
    let added = add_catalog_stage(&path_grouped(), "research", Some("small"));
    let steps = join_above(&move_stage(&added, "research", -1), "research");
    let lanes = build(&["implement", "docs", "research"]);
    assert_eq!(ids(&steps)[2], small(vec![lanes, sid("testing")]));
    assert_eq!(
        join_problem(&steps, "docs").as_deref(),
        Some("It runs alongside other stages already")
    );
}

#[test]
fn takes_a_lane_of_a_path_group_out_and_folds_a_group_of_one() {
    let alone = run_alone(&path_grouped(), "implement");
    let items = vec![sid("docs"), sid("implement"), sid("testing")];
    assert_eq!(ids(&alone)[2], small(items));
    let removed = remove_stage(&path_grouped(), "implement");
    assert_eq!(ids(&removed)[2], small(vec![sid("docs"), sid("testing")]));
}

#[test]
fn drags_a_lane_out_of_a_path_group_into_the_path_or_to_the_top() {
    let in_path = drop_stage(&path_grouped(), "docs", "testing", Side::After);
    let items = vec![sid("implement"), sid("testing"), sid("docs")];
    assert_eq!(ids(&in_path)[2], small(items));
    let out = drop_stage(&path_grouped(), "docs", "release", Side::Before);
    let rest = ids(&out)[2..].to_vec();
    let branch = small(vec![sid("implement"), sid("testing")]);
    assert_eq!(rest, vec![branch, sid("docs"), sid("release")]);
}

#[test]
fn drops_a_stage_into_a_path_group_or_next_to_it() {
    let joined = drop_stage(&path_grouped(), "plan", "docs", Side::After);
    let lanes = build(&["implement", "docs", "plan"]);
    assert_eq!(ids(&joined)[1], small(vec![lanes, sid("testing")]));
    let beside = drop_stage(&path_grouped(), "review", "docs", Side::Before);
    let items = vec![sid("review"), build(&["implement", "docs"]), sid("testing")];
    assert_eq!(ids(&beside)[2], branch_refs(items, Vec::new()));
}

#[test]
fn reorders_the_lanes_of_a_path_group_without_breaking_it() {
    let steps = drop_stage(&path_grouped(), "docs", "implement", Side::Before);
    let lanes = build(&["docs", "implement"]);
    assert_eq!(ids(&steps)[2], small(vec![lanes, sid("testing")]));
}

#[test]
fn moves_a_lane_and_a_whole_group_of_a_path() {
    let swapped = move_stage(&path_grouped(), "implement", 1);
    let lanes = build(&["docs", "implement"]);
    assert_eq!(ids(&swapped)[2], small(vec![lanes, sid("testing")]));
    let left = move_stage(&path_grouped(), "implement", -1);
    let items = vec![sid("implement"), sid("docs"), sid("testing")];
    assert_eq!(ids(&left)[2], small(items));
    let down = move_stage(&path_grouped(), "build", 1);
    let lanes = build(&["implement", "docs"]);
    assert_eq!(ids(&down)[2], small(vec![sid("testing"), lanes]));
    let up = move_stage(&path_grouped(), "build", -1);
    let lanes = build(&["implement", "docs"]);
    assert_eq!(ids(&up)[2], lanes);
    assert_eq!(ids(&up)[3], small(vec![sid("testing")]));
}

#[test]
fn keeps_every_id_once_after_any_move_in_a_path_group() {
    for id in ["implement", "docs", "build", "testing"] {
        for direction in [-1, 1] {
            let moved = move_stage(&path_grouped(), id, direction);
            assert_eq!(used_ids(&moved), used_ids(&path_grouped()));
            assert!(!has_nested_branch(&moved));
        }
    }
}
