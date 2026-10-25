//! Each route screen 3 calls: its method, path and body, and how its answer is read.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_client::ClientError;
use natamux_proto::stages::actions::{StageAction, StageActionInput};
use natamux_proto::tasks::task::TaskChanges;
use natamux_proto::tasks::template::TemplateInput;
use natamux_proto::workspaces::workspace::FanOutInput;
use serde_json::{Value, json};
use support::{exchange, ok};

const TASK: &str = r#"{"id":"T-0009","title":"Fix it","state":"review","stage":"spec","updatedAt":"2026-10-23T00:00:00.000Z"}"#;

fn task_answer() -> String {
    format!(r#"{{"task":{TASK}}}"#)
}

#[test]
fn sends_a_stage_action_with_only_the_fields_that_are_set() {
    let (task, sent) = ok("POST /tasks/T-0009/stage", &task_answer(), |c| {
        c.stage_action("T-0009", StageActionInput::new(StageAction::Approve))
    });
    assert_eq!(task.stage, "spec");
    assert_eq!(sent, json!({"action": "approve"}));

    let input = StageActionInput {
        action: StageAction::Reopen,
        stage: None,
        to: Some("spec".into()),
        reason: Some("Add the error cases".into()),
    };
    let (_, sent) = ok("POST /tasks/T-0009/stage", &task_answer(), |c| {
        c.stage_action("T-0009", input)
    });
    assert_eq!(
        sent,
        json!({"action": "reopen", "to": "spec", "reason": "Add the error cases"})
    );
}

#[test]
fn a_refused_stage_action_gives_the_daemons_message() {
    let (answer, _) = exchange(
        "POST /tasks/T-0009/stage",
        "409 Conflict",
        r#"{"error":"conflict","message":"spec is not waiting"}"#,
        |c| c.stage_action("T-0009", StageActionInput::new(StageAction::Skip)),
    );
    assert_eq!(
        answer.unwrap_err(),
        ClientError::Status {
            status: 409,
            message: "spec is not waiting".into()
        }
    );
}

#[test]
fn puts_the_workflow_ci_and_skip_marks_with_the_stage_and_on() {
    let answer = task_answer();
    let (task, sent) = ok("PUT /tasks/T-0009/workflow", &answer, |c| {
        c.set_workflow("T-0009", "plan", true)
    });
    assert_eq!(task.id, "T-0009");
    assert_eq!(sent, json!({"stage": "plan", "on": true}));
    let (_, sent) = ok("PUT /tasks/T-0009/ci", &answer, |c| {
        c.set_ci("T-0009", "testing", false)
    });
    assert_eq!(sent, json!({"stage": "testing", "on": false}));
    let (_, sent) = ok("PUT /tasks/T-0009/skip", &answer, |c| {
        c.set_skip("T-0009", "release", true)
    });
    assert_eq!(sent, json!({"stage": "release", "on": true}));
}

#[test]
fn reads_the_stage_files_of_a_task() {
    let body =
        r#"{"files":[{"name":"spec.md","size":120,"modifiedAt":"2026-10-23T01:00:00.000Z"}]}"#;
    let (files, sent) = ok("GET /tasks/T-0009/files", body, |c| {
        c.list_stage_files("T-0009")
    });
    assert_eq!((files[0].name.as_str(), files[0].size), ("spec.md", 120));
    assert_eq!(files[0].modified_at, "2026-10-23T01:00:00.000Z");
    assert_eq!(sent, Value::Null);
}

#[test]
fn patches_the_fields_that_are_set_and_renames_with_the_same_route() {
    let changes = TaskChanges {
        description: Some("More".into()),
        priority: Some("high".into()),
        labels: Some(vec!["bug".into()]),
        depends_on: Some(vec!["T-0001".into()]),
        ..TaskChanges::default()
    };
    let (task, sent) = ok("PATCH /tasks/T-0009", &task_answer(), |c| {
        c.update_task("T-0009", changes)
    });
    assert_eq!(task.id, "T-0009");
    assert_eq!(
        sent,
        json!({"description": "More", "priority": "high", "labels": ["bug"], "dependsOn": ["T-0001"]})
    );
    let (_, sent) = ok("PATCH /tasks/T-0009", &task_answer(), |c| {
        c.rename_task("T-0009", "New")
    });
    assert_eq!(sent, json!({"title": "New"}));
}

#[test]
fn saves_a_template_and_reads_the_one_that_comes_back() {
    let input = TemplateInput {
        name: "Bug".into(),
        title: "Fix: ".into(),
        description: "Steps".into(),
        priority: "high".into(),
        labels: vec!["bug".into()],
    };
    let body = r#"{"template":{"id":"bug","name":"Bug","title":"Fix: ","description":"Steps","priority":"high","labels":["bug"]}}"#;
    let (template, sent) = ok("POST /templates", body, |c| c.save_template(input));
    assert_eq!(template.id, "bug");
    assert_eq!(
        sent,
        json!({"name":"Bug","title":"Fix: ","description":"Steps","priority":"high","labels":["bug"]})
    );
}

#[test]
fn creates_an_issue_and_refreshes_the_pull_request_with_no_body() {
    let (task, sent) = ok("POST /tasks/T-0009/issue", &task_answer(), |c| {
        c.create_issue("T-0009")
    });
    assert_eq!((task.id.as_str(), sent), ("T-0009", Value::Null));
    let (task, sent) = ok(
        "POST /tasks/T-0009/pull-request/refresh",
        &task_answer(),
        |c| c.refresh_pull_request("T-0009"),
    );
    assert_eq!((task.id.as_str(), sent), ("T-0009", Value::Null));
}

#[test]
fn fans_out_and_reads_the_tries() {
    let body = r#"{"tasks":[{"id":"T-0010","title":"Fix it (try 2)","createdAt":"a","updatedAt":"b"},{"id":"T-0011","title":"Fix it (try 3)","createdAt":"a","updatedAt":"b"}]}"#;
    let (answer, sent) = exchange("POST /tasks/T-0009/fan-out", "201 Created", body, |c| {
        c.fan_out(
            "T-0009",
            FanOutInput {
                count: 2,
                start: false,
            },
        )
    });
    let tries = answer.unwrap();
    assert_eq!(tries.len(), 2);
    assert_eq!(tries[1].id, "T-0011");
    assert_eq!(sent, json!({"count": 2, "start": false}));
}
