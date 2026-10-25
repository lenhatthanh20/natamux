use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::Step;
use serde_json::{Value, json};

use super::*;
use crate::stages::editor_draft::{
    DEFAULT_CREATE_ISSUE, RawSettings, format_settings, parse_draft, same_settings, tracker_of,
    with_create_issue, with_steps,
};

fn object(value: Value) -> RawSettings {
    let Value::Object(map) = value else {
        return RawSettings::new();
    };
    map
}

fn tracker(raw: &Value) -> TrackerSettings {
    tracker_of(&object(raw.clone()))
}

#[test]
fn shares_one_draft_between_the_tabs_and_keeps_invalid_json_on_the_json_tab() {
    // A Stages edit shows in the text of the JSON tab.
    let mut steps = default_pipeline();
    if let Some(Step::Stage(stage)) = steps.first_mut() {
        stage.enabled = Some(false);
    }
    let text = format_settings(&with_steps(&RawSettings::new(), &steps));
    assert!(text.contains("\"enabled\": false"));
    // Text typed on the JSON tab shows on the Stages tab.
    let typed = r#"{ "stages": [{ "id": "implement", "kind": "agent", "gate": "auto" }] }"#;
    assert_eq!(parse_draft(typed).map(|draft| draft.steps.len()), Ok(1));
    // Broken text stays on the JSON tab and counts as one problem.
    let error = parse_draft(r#"{ "stages": ["#).unwrap_err();
    assert!(error.starts_with("This is not JSON"));
    assert_eq!(shown_tab(false, SettingsTab::Stages), SettingsTab::Json);
    assert_eq!(shown_tab(true, SettingsTab::Stages), SettingsTab::Stages);
    assert_eq!(error_count(false, 0), 1);
    assert_eq!(error_count(true, 2), 2);
}

#[test]
fn saves_the_choice_in_tracker_create_issue_and_keeps_the_other_keys() {
    let file =
        object(json!({ "x-team": "web", "tracker": { "type": "github", "linkInPr": false } }));
    let next = with_create_issue(&file, "on-spec-approved");
    assert_eq!(
        Value::Object(next),
        json!({
            "x-team": "web",
            "tracker": { "type": "github", "linkInPr": false, "createIssue": "on-spec-approved" }
        })
    );
}

#[test]
fn leaves_the_key_out_when_the_default_is_picked_again() {
    let file = object(json!({ "tracker": { "createIssue": "on-pr" } }));
    assert_eq!(
        Value::Object(with_create_issue(&file, DEFAULT_CREATE_ISSUE)),
        json!({})
    );
}

#[test]
fn keeps_the_type_and_link_in_pr_of_the_tracker_when_the_default_is_picked() {
    let file = object(
        json!({ "tracker": { "type": "github", "linkInPr": false, "createIssue": "on-pr" } }),
    );
    assert_eq!(
        Value::Object(with_create_issue(&file, DEFAULT_CREATE_ISSUE)),
        json!({ "tracker": { "type": "github", "linkInPr": false } })
    );
}

#[test]
fn has_nothing_to_save_when_the_choice_comes_back_to_the_start() {
    let start = "{}";
    let raw = object(json!({}));
    let on_pr = format_settings(&with_create_issue(&raw, "on-pr"));
    assert!(!same_settings(&on_pr, start));
    let draft = parse_draft(&on_pr).unwrap().raw;
    let back = format_settings(&with_create_issue(&draft, DEFAULT_CREATE_ISSUE));
    assert!(same_settings(&back, start));
}

#[test]
fn turns_the_choice_off_when_the_project_has_no_tracker() {
    assert!(issues_off(&tracker(
        &json!({ "tracker": { "type": "none" } })
    )));
    assert!(!issues_off(&tracker(&json!({}))));
    assert_eq!(
        ISSUES_OFF_NOTE,
        "Issues are off: tracker.type is none (JSON tab)"
    );
}

#[test]
fn shows_the_value_of_a_project_without_a_tracker_and_turns_on_with_one() {
    let off = tracker(&json!({ "tracker": { "type": "none", "createIssue": "on-pr" } }));
    assert_eq!(choice_index(&off.create_issue), 2);
    assert!(issues_off(&off));
    let on = tracker(&json!({ "tracker": { "type": "github", "createIssue": "on-pr" } }));
    assert!(!issues_off(&on));
}

#[test]
fn puts_issues_between_stages_and_json_and_drops_the_pipeline_note_there() {
    let labels: Vec<&str> = SettingsTab::ALL.iter().map(|tab| tab.label()).collect();
    assert_eq!(labels, ["Stages", "Issues", "JSON"]);
    assert!(shows_pipeline_note(SettingsTab::Stages));
    assert!(!shows_pipeline_note(SettingsTab::Issues));
    assert!(shows_pipeline_note(SettingsTab::Json));
    assert!(PIPELINE_NOTE.starts_with("New tasks use this pipeline."));
}

#[test]
fn checks_the_value_of_the_file_and_keeps_default_on_manual() {
    let on_pr = tracker(&json!({ "tracker": { "createIssue": "on-pr" } }));
    assert_eq!(choice_index(&on_pr.create_issue), 2);
    assert_eq!(CREATE_ISSUE_OPTIONS[0].0, DEFAULT_CREATE_ISSUE);
}

#[test]
fn moves_the_choice_with_the_arrow_keys() {
    assert_eq!(step_choice(0, "down"), Some(1));
    assert_eq!(step_choice(1, "up"), Some(0));
    assert_eq!(step_choice(3, "down"), Some(0));
    assert_eq!(step_choice(0, "x"), None);
    assert_eq!(
        step_tab(SettingsTab::Stages, "right"),
        Some(SettingsTab::Issues)
    );
    assert_eq!(
        step_tab(SettingsTab::Json, "right"),
        Some(SettingsTab::Stages)
    );
}

#[test]
fn keeps_the_json_tab_while_the_json_is_broken() {
    assert_eq!(shown_tab(false, SettingsTab::Issues), SettingsTab::Json);
}

#[test]
fn shows_a_change_made_on_the_json_tab() {
    let draft = parse_draft(r#"{ "tracker": { "createIssue": "never" } }"#).unwrap();
    assert_eq!(choice_index(&tracker_of(&draft.raw).create_issue), 3);
}

#[test]
fn shows_a_file_that_is_not_json_on_the_json_tab_and_blocks_save() {
    let error = parse_draft(r#"{ "stages": ["#).unwrap_err();
    assert_eq!(shown_tab(false, SettingsTab::Stages), SettingsTab::Json);
    assert!(json_alert(&error).starts_with("This is not JSON"));
    assert!(json_alert(&error).ends_with(" Fix it to use the Stages tab."));
    assert_eq!(
        crate::projects::settings_text::blocked_reason(false, error_count(false, 0), true, false),
        Some("Fix the JSON first")
    );
}

#[test]
fn turns_export_off_for_json_it_cannot_read_but_not_for_pipeline_problems() {
    // `"stages": []` parses; the problem is a pipeline error, not a parse error.
    assert!(parse_draft(r#"{ "stages": [] }"#).is_ok());
    assert_eq!(export_reason(true), None);
    assert!(parse_draft(r#"{ "stages": ["#).is_err());
    assert_eq!(export_reason(false), Some("Fix the JSON first"));
}

#[test]
fn asks_a_different_reset_question_for_text_that_cannot_be_read() {
    assert_eq!(reset_ask(false).text, Some("{}"));
    assert_eq!(
        reset_ask(false).question,
        "Replace the text with the default settings?"
    );
    assert_eq!(reset_ask(true).text, None);
    assert!(
        reset_ask(true)
            .question
            .starts_with("Put the default stages back?")
    );
}

#[test]
fn lists_a_problem_with_its_stage() {
    assert_eq!(
        problem_line(Some("spec"), "needs an output"),
        "spec: needs an output"
    );
    assert_eq!(problem_line(None, "no stage is on"), "no stage is on");
}
