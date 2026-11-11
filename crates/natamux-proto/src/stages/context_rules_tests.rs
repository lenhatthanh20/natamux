use serde_json::{Value, json};

use super::*;
use crate::stages::catalog::{default_pipeline, full_pipeline};
use crate::stages::pipeline_rules::{IssueLevel, PipelineIssue, pipeline_issues};

fn steps(value: Value) -> Vec<Step> {
    serde_json::from_value(value).unwrap()
}

fn issues(value: Value) -> Vec<PipelineIssue> {
    pipeline_issues(&steps(value))
}

fn error(stage: Option<&str>, message: &str) -> PipelineIssue {
    PipelineIssue {
        stage: stage.map(Into::into),
        message: message.into(),
        level: IssueLevel::Error,
    }
}

fn context() -> Value {
    json!({"id": "context", "kind": "context", "gate": "manual"})
}

fn agent(id: &str) -> Value {
    json!({"id": id, "kind": "agent", "gate": "auto"})
}

fn spec() -> Value {
    json!({"id": "spec", "kind": "agent", "gate": "manual", "output": "spec.md"})
}

fn with(stage: Value, field: &str, value: Value) -> Value {
    let mut stage = stage;
    stage[field] = value;
    stage
}

fn own_message(id: &str) -> String {
    format!(
        "\"{id}\" comes right after the context stage, so it runs with the task's agent in Terminal 1. Clear its own agent, arguments and terminal"
    )
}

fn kind_message(id: &str, what: &str) -> String {
    format!(
        "The stage right after the context stage must be an agent stage, and \"{id}\" is {what}"
    )
}

#[test]
fn finds_nothing_wrong_with_the_catalog_pipelines() {
    assert!(pipeline_issues(&default_pipeline()).is_empty());
    assert!(pipeline_issues(&full_pipeline()).is_empty());
}

#[test]
fn needs_a_context_stage() {
    assert_eq!(
        issues(json!([spec(), agent("implement")])),
        [error(None, NO_CONTEXT)]
    );
}

#[test]
fn keeps_the_context_stage_on() {
    let off = with(context(), "enabled", json!(false));
    assert_eq!(
        issues(json!([off, spec(), agent("implement")])),
        [error(Some("context"), CONTEXT_OFF)]
    );
}

#[test]
fn gives_one_message_for_a_context_in_the_wrong_place() {
    let found = issues(json!([agent("implement"), context()]));
    assert_eq!(
        found,
        [error(Some("context"), "The context stage must come first")]
    );
}

#[test]
fn keeps_the_stage_after_the_context_on_the_task_agent() {
    for (field, value) in [
        ("ownShell", json!(true)),
        ("agent", json!("codex")),
        ("args", json!("--model x")),
    ] {
        let found = issues(json!([context(), with(spec(), field, value), agent("implement")]));
        assert_eq!(found, [error(Some("spec"), &own_message("spec"))], "{field}");
    }
}

#[test]
fn needs_a_stage_that_starts_the_task_agent_after_the_context() {
    let kinds = [
        ("qa", "manual", "a manual stage"),
        ("review", "review", "a review stage"),
        ("ship", "release", "a release stage"),
    ];
    for (id, kind, what) in kinds {
        let stage = json!({"id": id, "kind": kind, "gate": "manual"});
        let found = issues(json!([context(), stage]));
        assert_eq!(found, [error(Some(id), &kind_message(id, what))], "{kind}");
    }
    let command = json!({"id": "load", "kind": "testing", "gate": "auto", "command": "make load"});
    assert_eq!(
        issues(json!([context(), command])),
        [error(
            Some("load"),
            &kind_message("load", "a testing stage with a command")
        )]
    );
    let testing = json!({"id": "load", "kind": "testing", "gate": "auto"});
    assert!(issues(json!([context(), testing])).is_empty());
}

#[test]
fn skips_a_stage_that_is_off() {
    let off = with(with(spec(), "ownShell", json!(true)), "enabled", json!(false));
    let plan = json!({"id": "plan", "kind": "agent", "gate": "manual", "output": "plan.md"});
    assert!(issues(json!([context(), off, plan, agent("implement")])).is_empty());
}

#[test]
fn checks_the_stage_of_a_group_that_keeps_terminal_1() {
    let test_spec = json!({"id": "test-spec", "kind": "agent", "gate": "manual",
        "output": "test-spec.md", "ownShell": true, "agent": "copilot"});
    let group =
        |implement: Value| json!({"id": "build", "parallel": [test_spec.clone(), implement]});
    assert!(issues(json!([context(), group(agent("implement"))])).is_empty());
    let own = with(agent("implement"), "agent", json!("codex"));
    assert_eq!(
        issues(json!([context(), group(own)])),
        [error(Some("implement"), &own_message("implement"))]
    );
}

#[test]
fn checks_the_first_stage_of_each_path_of_a_branch_after_the_context() {
    let when = json!({"type": "command", "run": "true"});
    let branch = json!({"id": "size", "paths": [
        {"id": "big", "when": when, "stages": [with(spec(), "ownShell", json!(true))]},
        {"id": "small", "stages": [agent("implement")]}]});
    assert_eq!(
        issues(json!([context(), branch.clone()])),
        [error(Some("spec"), &own_message("spec"))]
    );
    // A branch later in the pipeline is not right after the context.
    assert!(issues(json!([context(), agent("first"), branch])).is_empty());
}

#[test]
fn names_the_stages_after_the_context() {
    assert_eq!(after_context_stages(&default_pipeline()), ["spec"]);
    assert!(after_context_stages(&steps(json!([spec(), agent("implement")]))).is_empty());
}
