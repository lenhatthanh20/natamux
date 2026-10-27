use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, StageKind};
use serde_json::{Value, json};

use super::*;
use crate::stages::editor_fixtures::stage_step;

fn object(value: Value) -> RawSettings {
    let Value::Object(map) = value else {
        return RawSettings::new();
    };
    map
}

fn keys(raw: &RawSettings) -> Vec<&str> {
    raw.keys().map(String::as_str).collect()
}

#[test]
fn reads_the_default_pipeline_when_the_file_has_no_stages() {
    let draft = parse_draft(r#"{ "tracker": { "type": "none" } }"#).unwrap();
    assert_eq!(draft.steps, default_pipeline());
}

#[test]
fn refuses_text_that_is_not_json_or_not_an_object() {
    let broken = parse_draft(r#"{ "stages": ["#).unwrap_err();
    assert!(broken.starts_with("This is not JSON"));
    assert_eq!(
        parse_draft("[]").unwrap_err(),
        "The settings are one JSON object, such as {}."
    );
}

#[test]
fn refuses_stages_the_list_cannot_draw() {
    let error = parse_draft(r#"{ "stages": [{ "id": "spec", "kind": "robot", "gate": "auto" }] }"#)
        .unwrap_err();
    assert!(error.starts_with("stages, item 1: each stage needs"));
}

#[test]
fn refuses_settings_outside_the_stages_that_break_the_rules() {
    let error = parse_draft(r#"{ "tracker": { "type": "jira" } }"#).unwrap_err();
    assert!(error.starts_with("tracker.type"));
}

#[test]
fn keeps_stages_with_a_wrong_field_value_for_the_list_to_name_the_problem() {
    let draft = parse_draft(
        r#"{ "stages": [{ "id": "spec", "kind": "agent", "gate": "auto", "output": "x" }] }"#,
    );
    assert!(draft.is_ok());
}

#[test]
fn refuses_stages_that_are_not_a_list() {
    assert_eq!(
        parse_draft(r#"{ "stages": {} }"#).unwrap_err(),
        "stages: give a list of stages, such as []."
    );
}

#[test]
fn leaves_the_stages_out_when_they_equal_the_default_and_keeps_every_other_key() {
    let raw = object(json!({ "team": "web", "stages": [], "tracker": { "type": "none" } }));
    let next = with_steps(&raw, &default_pipeline());
    assert_eq!(
        Value::Object(next),
        json!({ "team": "web", "tracker": { "type": "none" } })
    );
}

#[test]
fn keeps_the_place_of_the_stages_among_the_keys_or_puts_them_first() {
    let steps = vec![stage_step("implement", StageKind::Agent, Gate::Auto)];
    let placed = with_steps(&object(json!({ "a": 1, "stages": [], "b": 2 })), &steps);
    assert_eq!(keys(&placed), ["a", "stages", "b"]);
    let first = with_steps(&object(json!({ "a": 1 })), &steps);
    assert_eq!(keys(&first), ["stages", "a"]);
}

#[test]
fn reads_the_tracker_of_the_draft_with_the_defaults_for_what_it_leaves_out() {
    let tracker = tracker_of(&object(json!({})));
    assert_eq!(
        (
            tracker.kind.as_str(),
            tracker.create_issue.as_str(),
            tracker.link_in_pr
        ),
        ("github", "manual", true)
    );
    let tracker = tracker_of(&object(
        json!({ "tracker": { "type": "none", "createIssue": "on-pr" } }),
    ));
    assert_eq!(
        (
            tracker.kind.as_str(),
            tracker.create_issue.as_str(),
            tracker.link_in_pr
        ),
        ("none", "on-pr", true)
    );
}

#[test]
fn falls_back_to_the_defaults_for_a_tracker_it_cannot_read() {
    let tracker = tracker_of(&object(json!({ "tracker": { "createIssue": "always" } })));
    assert_eq!(tracker.create_issue, "manual");
    assert_eq!(
        tracker_of(&object(json!({ "tracker": "github" }))).kind,
        "github"
    );
}

#[test]
fn sets_when_to_open_an_issue_and_keeps_every_other_key() {
    let raw = object(json!({
        "team": "web",
        "tracker": { "type": "github", "linkInPr": false },
        "stages": []
    }));
    let next = with_create_issue(&raw, "on-spec-approved");
    assert_eq!(
        Value::Object(next.clone()),
        json!({
            "team": "web",
            "tracker": { "type": "github", "linkInPr": false, "createIssue": "on-spec-approved" },
            "stages": []
        })
    );
    assert_eq!(keys(&next), ["team", "tracker", "stages"]);
    assert_eq!(
        Value::Object(with_create_issue(
            &object(json!({ "team": "web" })),
            "on-pr"
        )),
        json!({ "team": "web", "tracker": { "createIssue": "on-pr" } })
    );
}

#[test]
fn keeps_keys_of_the_tracker_it_does_not_know() {
    assert_eq!(
        Value::Object(with_create_issue(
            &object(json!({ "tracker": { "foo": 1 } })),
            "never"
        )),
        json!({ "tracker": { "foo": 1, "createIssue": "never" } })
    );
    let both = object(json!({ "tracker": { "foo": 1, "createIssue": "never" } }));
    assert_eq!(
        Value::Object(with_create_issue(&both, "manual")),
        json!({ "tracker": { "foo": 1 } })
    );
}

#[test]
fn leaves_the_default_out_and_a_tracker_with_nothing_left() {
    let none = object(json!({ "tracker": { "type": "none", "createIssue": "on-pr" } }));
    assert_eq!(
        Value::Object(with_create_issue(&none, "manual")),
        json!({ "tracker": { "type": "none" } })
    );
    let only = object(json!({ "team": "web", "tracker": { "createIssue": "on-pr" } }));
    assert_eq!(
        Value::Object(with_create_issue(&only, "manual")),
        json!({ "team": "web" })
    );
}

#[test]
fn compares_the_settings_not_the_text() {
    assert!(same_settings(
        r#"{"a":1,"b":[1,2]}"#,
        "{\n  \"b\": [1, 2],\n  \"a\": 1\n}"
    ));
    assert!(!same_settings(r#"{"a":1}"#, r#"{"a":2}"#));
    assert!(same_settings(r#"{"a":"#, r#"{"a":"#));
    assert!(!same_settings(r#"{"a":"#, r#"{"a":1}"#));
}

#[test]
fn c15_keeps_the_keys_of_the_new_file_that_the_person_did_not_change() {
    let start =
        object(json!({ "tracker": { "type": "github" }, "agents": { "permissionMode": "auto" } }));
    let stages = json!([{ "id": "implement", "kind": "agent", "gate": "auto" }]);
    let mut draft = start.clone();
    draft.insert("stages".into(), stages.clone());
    let now = object(json!({
        "tracker": { "type": "none" },
        "agents": { "permissionMode": "auto" },
        "team": "web"
    }));
    assert_eq!(
        Value::Object(merge_changes(&start, &draft, &now)),
        json!({
            "tracker": { "type": "none" },
            "agents": { "permissionMode": "auto" },
            "team": "web",
            "stages": stages
        })
    );
}

#[test]
fn c15_removes_a_key_the_person_removed_and_takes_the_value_of_the_draft_on_a_clash() {
    let start = object(json!({ "stages": [], "tracker": { "type": "github" } }));
    let draft = object(json!({ "tracker": { "type": "none" } }));
    let now = object(json!({
        "stages": [{ "id": "spec" }],
        "tracker": { "type": "github", "linkInPr": false }
    }));
    assert_eq!(
        Value::Object(merge_changes(&start, &draft, &now)),
        json!({ "tracker": { "type": "none" } })
    );
}

#[test]
fn reads_the_most_send_backs_of_the_draft() {
    assert_eq!(max_send_backs_of(&object(json!({}))), None);
    let three = object(json!({ "agents": { "maxSendBacks": 3 } }));
    assert_eq!(max_send_backs_of(&three), Some(3));
    let bad = object(json!({ "agents": { "maxSendBacks": "3" } }));
    assert_eq!(max_send_backs_of(&bad), None);
}

#[test]
fn writes_the_most_send_backs_and_keeps_the_other_keys_in_place() {
    let start = object(json!({ "agents": { "permissionMode": "ask" }, "team": "web" }));
    let set = with_max_send_backs(&start, Some(2));
    assert_eq!(
        Value::Object(set.clone()),
        json!({ "agents": { "permissionMode": "ask", "maxSendBacks": 2 }, "team": "web" })
    );
    assert_eq!(keys(&set), ["agents", "team"]);
    assert_eq!(with_max_send_backs(&set, None), start);
}

#[test]
fn drops_agents_when_no_limit_leaves_it_empty() {
    let start = object(json!({ "team": "web" }));
    let set = with_max_send_backs(&start, Some(0));
    assert_eq!(
        Value::Object(set.clone()),
        json!({ "team": "web", "agents": { "maxSendBacks": 0 } })
    );
    assert_eq!(with_max_send_backs(&set, None), start);
}

#[test]
fn reads_and_writes_the_idle_pause_minutes_beside_the_send_backs() {
    let start = object(json!({ "agents": { "maxSendBacks": 2 } }));
    assert_eq!(pause_idle_of(&start), None);

    let set = with_pause_idle(&start, Some(0));
    assert_eq!(
        Value::Object(set.clone()),
        json!({ "agents": { "maxSendBacks": 2, "pauseIdleAfterMinutes": 0 } })
    );
    assert_eq!(pause_idle_of(&set), Some(0));
    assert_eq!(with_pause_idle(&set, None), start);
    let bad = object(json!({ "agents": { "pauseIdleAfterMinutes": "30" } }));
    assert_eq!(pause_idle_of(&bad), None);
}

#[test]
fn checks_the_id_rule() {
    for id in ["a", "load-test", "1st-check", "a1-b2"] {
        assert!(fits_id_rule(id), "{id}");
    }
    for id in [
        "",
        "-load",
        "load-",
        "load--test",
        "load_test",
        "Load",
        "a b",
    ] {
        assert!(!fits_id_rule(id), "{id}");
    }
}

#[test]
fn formats_with_two_spaces() {
    let raw = json!({ "a": 1 }).as_object().cloned().unwrap_or_default();
    assert_eq!(format_settings(&raw), "{\n  \"a\": 1\n}");
}
