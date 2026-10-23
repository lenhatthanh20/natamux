use natamux_proto::stages::schema::StageKind;

use super::*;
use crate::stages::editor_draft::parse_draft;
use crate::stages::editor_fixtures::branched;

#[test]
fn reads_a_branch_from_the_json_and_refuses_one_the_list_cannot_draw() {
    let text = serde_json::json!({ "stages": branched() }).to_string();
    let draft = parse_draft(&text).unwrap();
    assert_eq!(draft.steps, branched());
    let no_stages = parse_draft(
        r#"{ "stages": [{ "id": "after-plan", "paths": [{ "id": "small" }, { "id": "other", "stages": [] }] }] }"#,
    );
    assert!(
        no_stages
            .unwrap_err()
            .starts_with("stages, item 1: a branch needs an id and paths")
    );
    let group = parse_draft(
        r#"{ "stages": [{ "id": "b", "paths": [{ "id": "p", "stages": [{ "id": "g", "parallel": [] }] }] }] }"#,
    );
    assert!(group.is_err());
}

#[test]
fn keeps_a_path_with_no_stage_and_a_condition_with_a_wrong_value_for_the_issues_to_name() {
    let draft = parse_draft(
        r#"{ "stages": [{ "id": "b", "paths": [{ "id": "p", "when": { "type": "command", "run": "" }, "stages": [] }, { "id": "q", "stages": [] }] }] }"#,
    );
    assert!(draft.is_ok());
}

#[test]
fn lists_every_id_stages_branches_and_paths() {
    let mut ids: Vec<String> = used_ids(&branched()).into_iter().collect();
    ids.sort();
    let mut want = [
        "after-plan",
        "context",
        "implement",
        "plan",
        "pr-review",
        "release",
        "review",
        "small",
        "testing",
    ];
    want.sort();
    assert_eq!(ids, want);
}

#[test]
fn finds_a_stage_of_a_path_its_path_and_its_place_in_it() {
    let steps = branched();
    let at = |index, path, slot| Place {
        index,
        lane: None,
        path,
        slot,
    };
    assert_eq!(find_place(&steps, "testing"), Some(at(2, Some(0), Some(1))));
    assert_eq!(find_place(&steps, "review"), Some(at(2, Some(1), Some(0))));
    assert_eq!(find_place(&steps, "after-plan"), Some(at(2, None, None)));
    assert_eq!(find_place(&steps, "small"), None);
    assert!(group_of(&steps, "testing").is_none());
    assert_eq!(
        find_stage(&steps, "review").map(|stage| stage.kind),
        Some(StageKind::Review)
    );
    assert_eq!(
        find_branch(&steps, "after-plan").map(|b| b.paths.len()),
        Some(2)
    );
    assert!(find_branch(&steps, "plan").is_none());
}
