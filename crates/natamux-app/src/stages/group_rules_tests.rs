use natamux_proto::stages::schema::{Gate, StageKind, Step, StepRef};

use super::*;
use crate::stages::editor_catalog::add_catalog_stage;
use crate::stages::editor_fixtures::{
    branch_refs, branched, group_ref, ids, path_grouped, sid, stage_step,
};

/// Context, Plan, Implement, Testing, a manual Review, Docs, Release: all at the top level.
fn plain() -> Vec<Step> {
    vec![
        stage_step("context", StageKind::Context, Gate::Manual),
        stage_step("plan", StageKind::Agent, Gate::Manual),
        stage_step("implement", StageKind::Agent, Gate::Auto),
        stage_step("testing", StageKind::Testing, Gate::Auto),
        stage_step("review", StageKind::Manual, Gate::Manual),
        stage_step("docs", StageKind::Agent, Gate::Auto),
        stage_step("release", StageKind::Release, Gate::Manual),
    ]
}

fn list(ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| id.to_string()).collect()
}

fn choice_ids(choices: &[(GroupPlace, Stage)]) -> Vec<&str> {
    choices.iter().map(|(_, stage)| stage.id.as_str()).collect()
}

fn group_named(steps: &[Step], id: &str) -> Option<Option<String>> {
    steps.iter().find_map(|step| match step {
        Step::Group(group) if group.id == id => Some(group.name.clone()),
        _ => None,
    })
}

#[test]
fn lists_agent_and_manual_stages_with_their_place() {
    let choices = group_choices(&branched(), None);
    assert_eq!(choice_ids(&choices), ["plan", "implement"]);
    assert_eq!(choices[0].0, GroupPlace::Top);
    assert_eq!(
        choices[1].0,
        GroupPlace::Path {
            id: "small".into(),
            name: "Small change".into()
        }
    );
    let top = group_choices(&plain(), None);
    assert_eq!(choice_ids(&top), ["plan", "implement", "review", "docs"]);
}

#[test]
fn leaves_out_stages_in_a_group_except_the_one_being_edited() {
    assert!(
        group_choices(&path_grouped(), None)
            .iter()
            .all(|(_, stage)| { stage.id != "implement" && stage.id != "docs" })
    );
    let edited = group_choices(&path_grouped(), Some("build"));
    assert_eq!(choice_ids(&edited), ["plan", "implement", "docs"]);
}

#[test]
fn tells_whether_stages_share_one_place() {
    assert!(same_place(&plain(), &list(&["plan", "docs"])));
    assert!(same_place(&plain(), &list(&["plan"])));
    assert!(!same_place(&branched(), &list(&["plan", "implement"])));
    let steps = add_catalog_stage(&branched(), "docs", Some("small"));
    assert!(same_place(&steps, &list(&["implement", "docs"])));
}

#[test]
fn makes_a_group_at_the_first_stage_in_list_order() {
    let steps = make_group(&plain(), &list(&["docs", "plan"]), "");
    let expected = vec![
        sid("context"),
        group_ref("plan-group", &["plan", "docs"]),
        sid("implement"),
        sid("testing"),
        sid("review"),
        sid("release"),
    ];
    assert_eq!(ids(&steps), expected);
    assert_eq!(group_named(&steps, "plan-group"), Some(None));
    let named = make_group(&plain(), &list(&["plan", "docs"]), " Build ");
    assert_eq!(
        group_named(&named, "plan-group"),
        Some(Some(" Build ".into()))
    );
}

#[test]
fn refuses_a_group_that_breaks_a_rule() {
    let steps = plain();
    for picked in [
        list(&["plan"]),
        list(&["plan", "testing"]),
        list(&["plan", "nope"]),
        list(&["plan", "plan"]),
    ] {
        assert_eq!(make_group(&steps, &picked, ""), steps, "{picked:?}");
    }
    let five = add_catalog_stage(&steps, "research", None);
    let picked = list(&["plan", "implement", "review", "docs", "research"]);
    assert_eq!(make_group(&five, &picked, ""), five);
    let mixed = branched();
    assert_eq!(make_group(&mixed, &list(&["plan", "implement"]), ""), mixed);
}

#[test]
fn makes_a_group_inside_a_path() {
    let steps = add_catalog_stage(&branched(), "docs", Some("small"));
    let grouped = make_group(&steps, &list(&["docs", "implement"]), "");
    let small = vec![
        group_ref("implement-group", &["implement", "docs"]),
        sid("testing"),
    ];
    assert_eq!(ids(&grouped)[2], branch_refs(small, vec![sid("review")]));
}

#[test]
fn edits_the_members_and_name_of_a_group_in_place() {
    let steps = make_group(&plain(), &list(&["plan", "implement"]), "");
    let edited = edit_group(&steps, "plan-group", &list(&["docs", "plan"]), "Think");
    let expected = vec![
        sid("context"),
        group_ref("plan-group", &["plan", "docs"]),
        sid("implement"),
        sid("testing"),
        sid("review"),
        sid("release"),
    ];
    assert_eq!(ids(&edited), expected);
    assert_eq!(
        group_named(&edited, "plan-group"),
        Some(Some("Think".into()))
    );
    let cleared = edit_group(&edited, "plan-group", &list(&["plan", "docs"]), "  ");
    assert_eq!(group_named(&cleared, "plan-group"), Some(None));
}

#[test]
fn edits_a_group_inside_a_path() {
    let steps = add_catalog_stage(&path_grouped(), "research", Some("small"));
    let edited = edit_group(&steps, "build", &list(&["research", "docs"]), "");
    let small = vec![
        group_ref("build", &["docs", "research"]),
        sid("implement"),
        sid("testing"),
    ];
    assert_eq!(ids(&edited)[2], branch_refs(small, vec![sid("review")]));
}

#[test]
fn refuses_an_edit_down_to_one_stage_or_from_another_place() {
    let steps = path_grouped();
    assert_eq!(edit_group(&steps, "build", &list(&["docs"]), ""), steps);
    let picked = list(&["implement", "docs", "plan"]);
    assert_eq!(edit_group(&steps, "build", &picked, ""), steps);
    assert_eq!(
        edit_group(&steps, "nope", &list(&["implement", "docs"]), ""),
        steps
    );
}

#[test]
fn splits_a_group_into_stages_in_order() {
    let steps = make_group(&plain(), &list(&["plan", "review"]), "");
    let split = split_group(&steps, "plan-group");
    let expected: Vec<StepRef> = ["context", "plan", "review", "implement", "testing", "docs"]
        .iter()
        .map(|id| sid(id))
        .chain([sid("release")])
        .collect();
    assert_eq!(ids(&split), expected);
    let in_path = split_group(&path_grouped(), "build");
    let small = vec![sid("implement"), sid("docs"), sid("testing")];
    assert_eq!(ids(&in_path)[2], branch_refs(small, vec![sid("review")]));
}
