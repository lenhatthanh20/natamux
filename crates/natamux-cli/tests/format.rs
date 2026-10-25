//! Ports of `packages/cli/src/__tests__/format.test.ts`, and a check of the help text against the
//! TypeScript one.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use natamux_cli::format::{DEFAULT_URL, format_task, usage};
use natamux_proto::events::task_event::{EVENT_SOURCES, sendable_event_types};
use natamux_proto::tasks::task::{TASK_PRIORITIES, TASK_STATES, Task, TaskContext};
use serde_json::json;

fn task() -> Task {
    serde_json::from_value(json!({
        "id": "T-0007",
        "title": "Export tasks",
        "createdAt": "2026-10-09T09:00:00.000Z",
        "updatedAt": "2026-10-09T09:00:00.000Z",
        "stage": "spec",
        "stages": ["context", "spec"],
        "context": {"source": "github-issue", "ref": "#42"},
        "issue": {"provider": "github", "key": "#42", "url": "https://github.com/o/r/issues/42"},
    }))
    .unwrap()
}

fn assert_has(text: &str, part: &str) {
    assert!(text.contains(part), "{part:?} not in {text:?}");
}

#[test]
fn shows_the_stage_the_issue_the_context_and_the_repository() {
    let text = format_task(&task());
    assert_has(&text, "Stage:      spec");
    assert_has(&text, "Issue:      #42 https://github.com/o/r/issues/42");
    assert_has(&text, "Context:    github-issue #42");
    assert_has(&text, "Repo:       default");
}

#[test]
fn shows_a_dash_for_a_missing_issue_and_context() {
    let text = format_task(&Task {
        issue: None,
        context: None,
        ..task()
    });
    assert_has(&text, "Issue:      -");
    assert_has(&text, "Context:    -");
}

#[test]
fn names_the_skill_of_the_context_picked_or_from_the_settings() {
    let picked = TaskContext {
        source: "skill".into(),
        skill: Some("jira-ticket".into()),
        reference: "ABC-1".into(),
        ..TaskContext::default()
    };
    let text = format_task(&Task {
        context: Some(picked),
        ..task()
    });
    assert_has(&text, "Context:    jira-ticket (skill) ABC-1");

    let configured = TaskContext {
        source: "skill".into(),
        name: Some("Linear".into()),
        reference: "LIN-7".into(),
        ..TaskContext::default()
    };
    let text = format_task(&Task {
        context: Some(configured),
        ..task()
    });
    assert_has(&text, "Context:    Linear (skill) LIN-7");
}

#[test]
fn usage_matches_the_typescript_text() {
    let source = include_str!("fixtures/format.ts");
    let start_mark = "export const USAGE = `";
    let start = source.find(start_mark).unwrap() + start_mark.len();
    let template = &source[start..];
    let template = &template[..template.find('`').unwrap()];
    let sendable: Vec<&str> = sendable_event_types().collect();
    let expected = template
        .replace("${TASK_PRIORITIES.join(', ')}", &TASK_PRIORITIES.join(", "))
        .replace("${TASK_STATES.join(', ')}", &TASK_STATES.join(", "))
        .replace("${EVENT_SOURCES.join(', ')}", &EVENT_SOURCES.join(", "))
        .replace("${SENDABLE_EVENT_TYPES.join(', ')}", &sendable.join(", "))
        .replace("${DEFAULT_URL}", DEFAULT_URL);
    assert!(
        !expected.contains("${"),
        "a placeholder is left in the template"
    );
    assert_eq!(usage(), expected);
}
