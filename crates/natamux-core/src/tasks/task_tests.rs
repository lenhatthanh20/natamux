//! Ports `packages/core/src/tasks/__tests__/task.test.ts`.

use serde_json::{Map, Value, json};

use super::*;

fn minimal() -> Value {
    json!({
        "id": "T-42",
        "title": "Fix timezone bug in reminders",
        "createdAt": "2026-10-08T09:00:00Z",
        "updatedAt": "2026-10-08T09:00:00Z"
    })
}

/// The minimal task with these fields set on top.
fn with(extra: Value) -> Value {
    let mut task = minimal();
    for (key, value) in extra.as_object().unwrap() {
        task[key] = value.clone();
    }
    task
}

fn parse(extra: Value) -> Result<Task, ZodError> {
    validate_task(&with(extra))
}

fn ok(extra: Value) -> Task {
    parse(extra).unwrap()
}

#[test]
fn fills_defaults_for_a_minimal_task() {
    let mut expected = minimal();
    for (key, value) in json!({
        "description": "", "state": "draft", "priority": "normal", "labels": [],
        "agent": null, "branch": null, "workspace": null, "pr": null, "prStatus": null,
        "parentId": null, "repo": null, "dependsOn": [], "stage": "implement",
        "stages": ["implement", "review", "release"], "lanes": {}, "paths": {},
        "stageSince": null, "backgroundSince": null, "workflowStages": [],
        "context": null, "issue": null
    })
    .as_object()
    .unwrap()
    {
        expected[key] = value.clone();
    }
    assert_eq!(serde_json::to_value(ok(json!({}))).unwrap(), expected);
}

#[test]
fn reads_the_workflow_stages_and_a_bad_value_as_none_so_a_hand_edited_file_still_loads() {
    assert_eq!(
        ok(json!({"workflowStages": ["plan"]})).workflow_stages,
        ["plan"]
    );
    for bad in [json!(null), json!("plan"), json!(3), json!(["Not An Id"])] {
        assert!(
            ok(json!({"workflowStages": bad}))
                .workflow_stages
                .is_empty()
        );
    }
}

#[test]
fn reads_the_stages_checked_on_ci_and_a_bad_value_as_none() {
    assert_eq!(
        ok(json!({"ciStages": ["implement"]})).ci_stages,
        Some(vec!["implement".to_string()])
    );
    assert_eq!(ok(json!({})).ci_stages, None);
    assert_eq!(ok(json!({"ciStages": "implement"})).ci_stages, None);
}

#[test]
fn reads_the_stages_marked_to_skip_and_a_bad_value_as_none() {
    assert_eq!(
        ok(json!({"skipStages": ["testing"]})).skip_stages,
        Some(vec!["testing".to_string()])
    );
    assert_eq!(ok(json!({})).skip_stages, None);
    assert_eq!(ok(json!({"skipStages": "testing"})).skip_stages, None);
}

#[test]
fn reads_whether_the_task_is_left_to_a_person_and_a_bad_value_as_not() {
    assert_eq!(ok(json!({"withoutAgent": true})).without_agent, Some(true));
    assert_eq!(ok(json!({})).without_agent, None);
    for bad in [json!(false), json!("x"), json!(1)] {
        assert_eq!(ok(json!({"withoutAgent": bad})).without_agent, None);
    }
}

#[test]
fn reads_the_path_taken_at_each_branch_and_the_paths_of_a_branch_in_its_steps() {
    let stages = json!([
        "plan",
        {"id": "after-plan", "paths": [
            {"id": "small", "stages": ["implement", "testing"]},
            {"id": "pr-review", "stages": ["review"]}
        ]},
        "release"
    ]);
    let task = ok(json!({"stages": stages, "paths": {"after-plan": "small"}}));
    assert_eq!(Value::Array(task.stages), stages);
    assert_eq!(Value::Object(task.paths), json!({"after-plan": "small"}));
    assert!(ok(json!({"stages": stages})).paths.is_empty());
}

#[test]
fn trims_the_title() {
    assert_eq!(ok(json!({"title": "  Fix it  "})).title, "Fix it");
}

#[test]
fn rejects_the_empty_title() {
    for title in ["", "   "] {
        assert!(parse(json!({"title": title})).is_err(), "{title:?}");
    }
}

#[test]
fn rejects_the_id() {
    for id in ["42", "T42", "t-42", "T-", "T-4a", "A1-2", "-1"] {
        assert!(parse(json!({"id": id})).is_err(), "{id:?}");
    }
}

#[test]
fn accepts_an_id_of_32_characters_and_rejects_a_longer_one() {
    let id = format!("{}-12", "A".repeat(29));
    assert_eq!(ok(json!({"id": id})).id, id);
    assert!(parse(json!({"id": format!("A{id}")})).is_err());
}

#[test]
fn names_both_id_forms_when_an_id_is_not_valid() {
    let error = parse(json!({"id": "T42"})).unwrap_err();
    assert_eq!(error.0[0].message(), "Task id looks like T-42 or ABC-42");
}

#[test]
fn accepts_the_id() {
    for id in ["T-1", "T-0001", "ABC-1", "TASK-0005"] {
        assert_eq!(ok(json!({"id": id})).id, id);
    }
}

#[test]
fn rejects_an_unknown_state_and_priority() {
    assert!(parse(json!({"state": "finished"})).is_err());
    assert!(parse(json!({"priority": "asap"})).is_err());
}

#[test]
fn rejects_a_dependency_that_is_not_a_task_id() {
    assert!(parse(json!({"dependsOn": ["nope"]})).is_err());
}

#[test]
fn keeps_the_name_of_the_context_source() {
    let context = json!({"source": "skill", "name": "Linear", "ref": "LIN-7"});
    let task = ok(json!({"context": context}));
    assert_eq!(serde_json::to_value(task.context).unwrap(), context);
}

#[test]
fn keeps_the_skill_picked_for_the_task_and_parses_a_context_without_one() {
    let picked = json!({"source": "skill", "skill": "jira-ticket", "ref": "ABC-1"});
    let task = ok(json!({"context": picked}));
    assert_eq!(serde_json::to_value(task.context).unwrap(), picked);
    let plain = json!({"source": "skill", "name": "Ticket", "ref": "ABC-1"});
    let task = ok(json!({"context": plain}));
    assert_eq!(serde_json::to_value(task.context).unwrap(), plain);
    let empty = json!({"source": "skill", "skill": " ", "ref": "ABC-1"});
    assert!(parse(json!({"context": empty})).is_err());
}

#[test]
fn trims_the_picked_skill_and_takes_1_to_200_characters_of_text() {
    let context = |skill: Value| {
        let value = json!({"source": "skill", "skill": skill, "ref": "REC-1"});
        let mut issues = Issues::default();
        let parsed = task_context(&value, &[], &mut issues);
        issues.is_empty().then_some(parsed).flatten()
    };
    assert_eq!(
        context(json!("  ticket-reader  ")).unwrap()["skill"],
        "ticket-reader"
    );
    assert!(context(json!("a".repeat(200))).is_some());
    for skill in [
        json!(""),
        json!("   "),
        json!("a".repeat(201)),
        json!(42),
        json!(null),
    ] {
        assert!(context(skill.clone()).is_none(), "{skill}");
    }
}

#[test]
fn takes_a_title_of_1_to_400_characters() {
    assert!(parse(json!({"title": "a".repeat(400)})).is_ok());
    for title in ["".to_string(), "   ".into(), "a".repeat(401)] {
        assert!(parse(json!({"title": title})).is_err());
    }
}

#[test]
fn keeps_a_pr_at_an_http_or_https_url() {
    for pr in [
        "https://github.com/o/r/pull/1",
        "http://git.example.com/o/r/pull/1",
    ] {
        assert_eq!(ok(json!({"pr": pr})).pr.as_deref(), Some(pr));
    }
}

// A click on the link would open the handler of its scheme on the user's machine. A bad stored
// value drops the link, so the task list still loads.
#[test]
fn drops_a_pr_at_a_url_that_is_not_http_or_https() {
    for pr in [
        "vscode://file/etc/passwd",
        "file:///etc/passwd",
        "javascript:alert(1)",
        "pull 12",
    ] {
        assert_eq!(ok(json!({"pr": pr})).pr, None, "{pr}");
    }
}

#[test]
fn drops_an_issue_link_with_a_url_that_is_not_http_or_https() {
    let issue = json!({"provider": "jira", "key": "ABC-1", "url": "vscode://file/etc/passwd"});
    assert_eq!(ok(json!({"issue": issue})).issue, None);
}

fn link_is_ok(url: &str) -> bool {
    let value = json!({"provider": "jira", "key": "ABC-1", "url": url});
    let mut issues = Issues::default();
    issue_link(&value, &[], &mut issues).is_some() && issues.is_empty()
}

#[test]
fn issue_link_takes_http_and_https_urls() {
    for url in [
        "https://jira.example.com/browse/ABC-1",
        "http://10.0.0.5:8080/browse/ABC-1",
    ] {
        assert!(link_is_ok(url), "{url}");
    }
}

#[test]
fn issue_link_refuses_other_urls() {
    for url in [
        "vscode://file/etc/passwd",
        "file:///etc/passwd",
        "javascript:alert(1)",
    ] {
        assert!(!link_is_ok(url), "{url}");
    }
}

fn map_of(value: Value) -> Map<String, Value> {
    value.as_object().unwrap().clone()
}

#[test]
fn puts_an_old_task_at_the_stage_of_its_state() {
    for (state, stage) in [
        ("review", "review"),
        ("done", "release"),
        ("running", "implement"),
        ("draft", "implement"),
    ] {
        assert_eq!(
            Value::Object(upgrade_task_fields(map_of(json!({"state": state})))),
            json!({"state": state, "stage": stage, "stages": ["implement", "review", "release"]})
        );
    }
}

#[test]
fn links_the_issue_an_old_task_was_imported_from() {
    let upgraded = upgrade_task_fields(map_of(json!({
        "state": "draft",
        "description": "Body\n\nImported from https://github.com/acme/app/issues/42"
    })));
    assert_eq!(
        upgraded["issue"],
        json!({"provider": "github", "key": "#42", "url": "https://github.com/acme/app/issues/42"})
    );
}

#[test]
fn leaves_a_task_that_has_a_stage_as_it_is() {
    let task = map_of(json!({"state": "review", "stage": "plan"}));
    assert_eq!(upgrade_task_fields(task.clone()), task);
}

#[test]
fn splits_a_title_that_starts_with_an_id() {
    for (title, id, rest) in [
        ("Task-0005: Fix bug xxx", "TASK-0005", "Fix bug xxx"),
        ("Task-0005 Fix bug xxx", "TASK-0005", "Fix bug xxx"),
        ("  abc-12 : Fix it ", "ABC-12", "Fix it"),
        ("abc-12 Fix it", "ABC-12", "Fix it"),
        ("Task-0005:Fix", "TASK-0005", "Fix"),
        ("aBc-12:Fix it", "ABC-12", "Fix it"),
        ("abc-12\tFix it", "ABC-12", "Fix it"),
        ("Task-5: a: b", "TASK-5", "a: b"),
        ("Task-5: fix T-12 parser", "TASK-5", "fix T-12 parser"),
        ("COVID-19 tracker", "COVID-19", "tracker"),
    ] {
        assert_eq!(
            split_task_id(title),
            (Some(id.to_string()), rest.to_string()),
            "{title:?}"
        );
    }
}

#[test]
fn leaves_a_title_without_an_id_as_a_plain_title() {
    for title in [
        "Fix bug xxx",
        "Fix T-12 parser",
        "Task-0005",
        "Task-0005:",
        " Task-0005 : ",
        "Task-0005:   ",
        "A-1x y",
        "T-12x parser",
        "abc-12, Fix it",
        "abc-12- Fix it",
        "abc-12.Fix it",
        "abc-12-3 x",
        "A1-2 x",
        "ABC12 x",
        "Tâsk-1: x",
    ] {
        assert_eq!(
            split_task_id(title),
            (None, title.trim().to_string()),
            "{title:?}"
        );
    }
}
