//! T-0373 (F6): an agent's `task list` and `task show` read the scoped body the daemon gives an
//! agent token: its own task in full, its subtasks as a short view without `updatedAt`.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_cli::run::EXIT_OK;
use serde_json::json;
use support::*;

fn subtask_view() -> serde_json::Value {
    json!({
        "id": "T-0003", "title": "Child work", "state": "running", "stage": "implement",
        "parentId": "T-0001", "pr": null, "prStatus": null, "stageSince": null,
        "branch": "natamux/t-0003",
    })
}

#[test]
fn task_list_prints_the_own_task_and_the_subtask_view() {
    let result = run(&["task", "list"], |_| {
        let own = serde_json::to_value(make_task_with(|task| task.id = "T-0001".into())).unwrap();
        json_reply(json!({ "tasks": [own, subtask_view()] }))
    });
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert!(result.stdout.contains("T-0001"), "{}", result.stdout);
    assert!(result.stdout.contains("Child work"), "{}", result.stdout);
}

#[test]
fn task_show_prints_a_subtask_view() {
    let result = run(&["task", "show", "T-0003"], |_| {
        json_reply(json!({ "task": subtask_view() }))
    });
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert!(
        result.stdout.contains("T-0003  Child work"),
        "{}",
        result.stdout
    );
    assert!(
        result.stdout.contains("Branch:     natamux/t-0003"),
        "{}",
        result.stdout
    );
}
