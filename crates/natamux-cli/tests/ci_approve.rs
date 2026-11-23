//! `natamux stage ci-approve`: a person approves the exact tree a Strict CI push sends (F4).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_cli::run::{EXIT_FAILED, EXIT_OK, EXIT_USAGE};
use serde_json::json;
use support::*;

const ARGV: [&str; 7] = [
    "stage",
    "ci-approve",
    "implement",
    "--task",
    "T-7",
    "--tree",
    "abc123",
];

#[test]
fn sends_the_tree_to_the_approve_route_of_the_stage() {
    let result = run(&ARGV, |_| json_reply(json!({ "task": { "id": "T-7" } })));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert_eq!(
        result.lines(),
        ["POST http://127.0.0.1:4317/tasks/T-7/stages/implement/ci-approve"]
    );
    assert_eq!(result.calls[0].json().unwrap(), json!({ "tree": "abc123" }));
    assert_eq!(
        result.stdout,
        "Approved the CI push of implement on T-7 (tree abc123).\n"
    );
}

#[test]
fn says_why_another_tree_is_refused() {
    let refused = json!({ "error": "conflict", "message": "The tree to send is def456." });
    let result = run(&ARGV, move |_| json_status(409, refused.clone()));
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("The tree to send is def456."));
}

#[test]
fn needs_the_stage_the_task_and_the_tree() {
    for argv in [
        &["stage", "ci-approve", "--task", "T-7", "--tree", "a"][..],
        &["stage", "ci-approve", "implement", "--tree", "a"],
        &["stage", "ci-approve", "implement", "--task", "T-7"],
        &[
            "stage",
            "ci-approve",
            "implement",
            "--task",
            "bad id",
            "--tree",
            "a",
        ],
        &[
            "stage",
            "ci-approve",
            "implement",
            "extra",
            "--task",
            "T-7",
            "--tree",
            "a",
        ],
    ] {
        let result = run(argv, |_| json_reply(json!({})));
        assert_eq!(result.code, EXIT_USAGE, "{argv:?}");
        assert!(result.calls.is_empty());
    }
}
