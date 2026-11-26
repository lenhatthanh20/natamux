use super::*;

// A task as the daemon of 0.1.43 sends it, with fields this crate does not read.
const TASK: &str = r#"{
    "id": "T-0143", "title": "Rust migration phase 1", "description": "", "state": "needs-input",
    "priority": "normal", "labels": [], "agent": "claude-code", "branch": "natamux/t-0143",
    "workspace": "/w/T-0143", "pr": null, "prStatus": null, "dependsOn": [], "parentId": null,
    "repo": "natamux", "stage": "implement", "stages": ["spec", "implement"], "lanes": {},
    "paths": {}, "stageSince": null, "backgroundSince": null, "workflowStages": [],
    "context": null, "issue": null, "createdAt": "2026-10-22T00:53:31.924Z",
    "updatedAt": "2026-10-22T00:59:38.510Z", "aFieldFromLater": {"x": 1}
}"#;

#[test]
fn reads_a_task_and_ignores_other_fields() {
    let task: Task = serde_json::from_str(TASK).unwrap();
    assert_eq!(task.id, "T-0143");
    assert_eq!(task.state, TaskState::NeedsInput);
    assert_eq!(task.stage, "implement");
    assert_eq!(task.workspace.as_deref(), Some("/w/T-0143"));
    assert_eq!(task.parent_id, None);
    assert_eq!(task.updated_at, "2026-10-22T00:59:38.510Z");
    assert!(task.has_live_terminal());
}

#[test]
fn writes_every_field_back_in_the_schemas_order_and_drops_unknown_ones() {
    let task: Task = serde_json::from_str(TASK).unwrap();
    let written = serde_json::to_string(&task).unwrap();
    assert_eq!(
        written,
        concat!(
            r#"{"id":"T-0143","title":"Rust migration phase 1","description":"","#,
            r#""state":"needs-input","priority":"normal","labels":[],"agent":"claude-code","#,
            r#""branch":"natamux/t-0143","workspace":"/w/T-0143","pr":null,"prStatus":null,"#,
            r#""dependsOn":[],"parentId":null,"repo":"natamux","stage":"implement","#,
            r#""stages":["spec","implement"],"lanes":{},"paths":{},"stageSince":null,"#,
            r#""backgroundSince":null,"workflowStages":[],"context":null,"issue":null,"#,
            r#""createdAt":"2026-10-22T00:53:31.924Z","updatedAt":"2026-10-22T00:59:38.510Z"}"#
        )
    );
}

#[test]
fn keeps_the_daemons_order_of_lanes_and_optional_fields_when_set() {
    let text = TASK.replace(
        r#""lanes": {}"#,
        r#""lanes": {"tests": "running", "docs": "review"}, "ciStages": ["implement"]"#,
    );
    let task: Task = serde_json::from_str(&text).unwrap();
    let written = serde_json::to_string(&task).unwrap();
    assert!(written.contains(r#""lanes":{"tests":"running","docs":"review"}"#));
    assert!(written.contains(r#""workflowStages":[],"ciStages":["implement"],"context""#));
    assert!(!written.contains("skipStages"));
}

#[test]
fn fills_zods_defaults() {
    let task: Task =
        serde_json::from_str(r#"{"id":"T-1","title":"t","createdAt":"a","updatedAt":"b"}"#)
            .unwrap();
    assert_eq!(task.state, TaskState::Draft);
    assert_eq!(task.priority, "normal");
    assert_eq!(task.stage, "implement");
}

#[test]
fn task_ids_are_upper_case_letters_a_dash_and_digits() {
    for id in ["T-42", "ABC-1", "T-0001"] {
        assert!(is_task_id(id), "{id}");
    }
    for id in [
        "t-42",
        "T42",
        "T-",
        "-1",
        "T-4a",
        "TÉ-1",
        "T-1-2",
        &format!("T-{}", "1".repeat(31)),
    ] {
        assert!(!is_task_id(id), "{id}");
    }
}

#[test]
fn reads_and_writes_the_idle_state() {
    let task: Task = serde_json::from_str(&TASK.replace("needs-input", "idle")).unwrap();
    assert_eq!(task.state, TaskState::Idle);
    assert_eq!(serde_json::to_value(TaskState::Idle).unwrap(), "idle");
    assert_eq!(TaskState::Idle.name(), "idle");
}

#[test]
fn a_state_from_a_newer_daemon_is_unknown() {
    let task: Task = serde_json::from_str(&TASK.replace("needs-input", "paused")).unwrap();
    assert_eq!(task.state, TaskState::Unknown);
}

#[test]
fn finished_or_workspace_less_tasks_have_no_live_terminal() {
    let mut task: Task = serde_json::from_str(TASK).unwrap();
    task.state = TaskState::Done;
    assert!(!task.has_live_terminal());
    task.state = TaskState::Running;
    task.workspace = None;
    assert!(!task.has_live_terminal());
}

#[test]
fn reads_the_list_and_the_ticket() {
    let list: TaskList = serde_json::from_str(&format!(r#"{{"tasks":[{TASK}]}}"#)).unwrap();
    assert_eq!(list.tasks.len(), 1);
    let ticket: Ticket = serde_json::from_str(r#"{"ticket":"abc"}"#).unwrap();
    assert_eq!(ticket.ticket, "abc");
}

#[test]
fn inputs_send_only_what_is_set_in_the_routes_names() {
    let task = NewTaskInput {
        title: "Add subtask".into(),
        parent_id: Some("T-1".into()),
        repo: Some("natamux".into()),
    };
    assert_eq!(
        serde_json::to_string(&task).unwrap(),
        r#"{"title":"Add subtask","parentId":"T-1","repo":"natamux"}"#
    );
    let plain = NewTaskInput {
        title: "x".into(),
        ..Default::default()
    };
    assert_eq!(serde_json::to_string(&plain).unwrap(), r#"{"title":"x"}"#);
    let template = TemplateTaskInput {
        title: "x".into(),
        repo: Some("a".into()),
    };
    assert_eq!(
        serde_json::to_string(&template).unwrap(),
        r#"{"title":"x","repo":"a"}"#
    );
    let patch = TaskChanges {
        title: Some("New".into()),
        ..Default::default()
    };
    assert_eq!(serde_json::to_string(&patch).unwrap(), r#"{"title":"New"}"#);
    let more = TaskChanges {
        labels: Some(vec!["a".into()]),
        depends_on: Some(vec!["T-1".into()]),
        ..Default::default()
    };
    assert_eq!(
        serde_json::to_string(&more).unwrap(),
        r#"{"labels":["a"],"dependsOn":["T-1"]}"#
    );
}

#[test]
fn reads_the_delete_answer_and_the_task_answer() {
    let answer: DeleteTaskAnswer =
        serde_json::from_str(&format!(r#"{{"id":"T-2","tasks":[{TASK}]}}"#)).unwrap();
    assert_eq!(answer.id, "T-2");
    assert_eq!(answer.tasks.len(), 1);
    let made: TaskAnswer = serde_json::from_str(&format!(r#"{{"task":{TASK}}}"#)).unwrap();
    assert_eq!(made.task.id, "T-0143");
}

#[test]
fn reads_the_pull_request_status_of_a_task() {
    let mut task: Task = serde_json::from_str(TASK).unwrap();
    assert_eq!(task.pull_request_status(), None);
    let text = TASK.replace(
        r#""prStatus": null"#,
        r#""prStatus": {"state":"merged","checks":"passing","x":1}"#,
    );
    task = serde_json::from_str(&text).unwrap();
    let status = task.pull_request_status().unwrap();
    assert_eq!(
        (status.state.as_str(), status.checks.as_str()),
        ("merged", "passing")
    );
}
