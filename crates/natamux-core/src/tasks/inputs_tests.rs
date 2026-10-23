//! No TypeScript file to port: the texts are those of zod 4.6.7 for the schemas in `task.ts`,
//! `task-inputs.ts` and `routes/tasks.ts`.

use serde_json::{Map, Value, json};

use super::*;
use crate::json::to_js_string;

const EMPTY: &str = "Too small: expected string to have >=1 characters";
const NO_STRING: &str = "Invalid input: expected string, received undefined";

fn text(map: Map<String, Value>) -> String {
    to_js_string(&Value::Object(map))
}

fn failed<T: std::fmt::Debug>(result: Result<T, ZodError>) -> ZodError {
    result.unwrap_err()
}

#[test]
fn a_new_task_gets_the_defaults_in_the_order_of_the_schema() {
    let map = create_task_input(Some(&json!({"title": " Fix it ", "extra": 1}))).unwrap();
    assert_eq!(
        text(map),
        r#"{"title":"Fix it","description":"","priority":"normal","labels":[],"agent":null,"dependsOn":[],"parentId":null,"repo":null}"#
    );
}

#[test]
fn a_new_task_keeps_a_context_and_trims_its_ref() {
    let body = json!({"title": "x", "context": {"source": "skill", "ref": " a "}});
    let map = create_task_input(Some(&body)).unwrap();
    assert_eq!(map["context"], json!({"source": "skill", "ref": "a"}));
    let map = create_task_input(Some(&json!({"title": "x", "context": null}))).unwrap();
    assert_eq!(map["context"], Value::Null);
}

#[test]
fn a_new_task_names_each_field_that_is_wrong() {
    let error = failed(create_task_input(Some(&json!({
        "title": "  ", "priority": "asap", "labels": [""], "dependsOn": ["x"]
    }))));
    assert_eq!(
        error.describe(),
        format!(
            "title: {EMPTY}; priority: Invalid option: expected one of \
             \"low\"|\"normal\"|\"high\"|\"urgent\"; labels.0: {EMPTY}; \
             dependsOn.0: Task id looks like T-42 or ABC-42"
        )
    );
}

#[test]
fn the_issues_of_a_wrong_task_are_json_as_zod_writes_them() {
    let error = failed(create_task_input(Some(&json!({
        "title": "a".repeat(401), "dependsOn": ["x"]
    }))));
    assert_eq!(
        to_js_string(&error.issues()),
        concat!(
            r#"[{"origin":"string","code":"too_big","maximum":400,"inclusive":true,"path":["title"],"#,
            r#""message":"Too big: expected string to have <=400 characters"},"#,
            r#"{"origin":"string","code":"invalid_format","format":"regex","pattern":"/^[A-Z]+-\\d+$/","#,
            r#""path":["dependsOn",0],"message":"Task id looks like T-42 or ABC-42"}]"#
        )
    );
    let error = failed(create_task_input(None));
    assert_eq!(
        to_js_string(&error.issues()),
        r#"[{"expected":"object","code":"invalid_type","path":[],"message":"Invalid input: expected object, received undefined"}]"#
    );
}

#[test]
fn an_update_holds_only_the_keys_sent() {
    let map = update_task_input(Some(&json!({"labels": [" a "], "title": " T "}))).unwrap();
    assert_eq!(text(map), r#"{"title":"T","labels":["a"]}"#);
    assert!(update_task_input(Some(&json!({}))).unwrap().is_empty());
}

#[test]
fn an_update_refuses_a_key_it_does_not_have() {
    let error = failed(update_task_input(Some(
        &json!({"title": "x", "state": "done", "foo": 1}),
    )));
    assert_eq!(error.describe(), "Unrecognized keys: \"state\", \"foo\"");
    assert_eq!(
        to_js_string(&error.issues()),
        r#"[{"code":"unrecognized_keys","keys":["state","foo"],"path":[],"message":"Unrecognized keys: \"state\", \"foo\""}]"#
    );
}

#[test]
fn an_event_from_a_client_gets_an_empty_payload() {
    let map = apply_event_input(Some(&json!({"source": "human", "type": "task.readied"}))).unwrap();
    assert_eq!(
        text(map),
        r#"{"source":"human","type":"task.readied","payload":{}}"#
    );
}

#[test]
fn an_event_the_daemon_records_is_refused() {
    let error = failed(apply_event_input(Some(
        &json!({"source": "human", "type": "git.pushed"}),
    )));
    assert_eq!(
        to_js_string(&error.issues()),
        r#"[{"code":"custom","path":["type"],"message":"This event is recorded by the daemon, not sent by clients"}]"#
    );
}

#[test]
fn the_refusal_waits_when_another_field_is_wrong() {
    let error = failed(apply_event_input(Some(&json!({
        "source": "bot", "type": "git.pushed", "payload": 3
    }))));
    assert_eq!(
        error.describe(),
        "source: Invalid option: expected one of \"agent\"|\"human\"|\"git\"|\"ci\"|\"system\"; \
         payload: Invalid input: expected record, received number"
    );
}

#[test]
fn a_template_gets_the_defaults() {
    let map = template_input(Some(&json!({"name": " N ", "title": "T"}))).unwrap();
    assert_eq!(
        text(map),
        r#"{"name":"N","title":"T","description":"","priority":"normal","labels":[]}"#
    );
}

#[test]
fn a_template_names_each_field_that_is_wrong() {
    let error = failed(template_input(Some(&json!({}))));
    assert_eq!(
        error.describe(),
        format!("name: {NO_STRING}; title: {NO_STRING}")
    );
    let error = failed(template_input(Some(
        &json!({"name": "n", "title": "a".repeat(201)}),
    )));
    assert_eq!(
        error.describe(),
        "title: Too big: expected string to have <=200 characters"
    );
}

#[test]
fn a_task_from_a_template_has_a_title_and_maybe_a_repository() {
    let both = from_template_input(Some(&json!({"title": " T ", "repo": "my-repo"})));
    assert_eq!(
        both.unwrap(),
        ("T".to_string(), Some("my-repo".to_string()))
    );
    let none = from_template_input(Some(&json!({"title": "T", "repo": null})));
    assert_eq!(none.unwrap(), ("T".to_string(), None));
    let error = failed(from_template_input(Some(
        &json!({"title": "T", "repo": "Bad Repo"}),
    )));
    assert_eq!(
        error.describe(),
        "repo: A repository id is lower case words joined by \"-\""
    );
}

#[test]
fn attachments_are_optional_and_hold_a_name_and_data() {
    assert_eq!(attachments_body(&json!({})).unwrap(), vec![]);
    let body = json!({"attachments": [{"name": "a.png", "data": "QQ=="}]});
    assert_eq!(
        attachments_body(&body).unwrap(),
        [Upload {
            name: "a.png".into(),
            data: "QQ==".into()
        }]
    );
}

#[test]
fn attachments_name_each_field_that_is_wrong() {
    let error = failed(attachments_body(
        &json!({"attachments": [{"name": "", "data": 1}]}),
    ));
    assert_eq!(
        error.describe(),
        format!(
            "attachments.0.name: {EMPTY}; \
             attachments.0.data: Invalid input: expected string, received number"
        )
    );
}

#[test]
fn a_task_takes_at_most_20_attachments() {
    let files: Vec<Value> = (0..21)
        .map(|n| json!({"name": format!("{n}.txt"), "data": ""}))
        .collect();
    let error = failed(attachments_body(&json!({"attachments": files})));
    assert_eq!(
        error.describe(),
        "attachments: Too big: expected array to have <=20 items"
    );
}
