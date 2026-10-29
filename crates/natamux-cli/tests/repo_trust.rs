//! `natamux repo trust`: the commands of a project and their approval, for headless use.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_cli::run::{EXIT_FAILED, EXIT_OK, EXIT_USAGE};
use serde_json::{Value, json};
use support::*;

fn view(trusted: bool) -> Value {
    json!({
        "trusted": trusted,
        "hash": "ab12",
        "commands": [{ "where": "Stage test: check", "text": "pnpm test" }],
        "setup": [],
    })
}

#[test]
fn prints_the_commands_and_the_line_that_approves_them() {
    let result = run(&["repo", "trust", "my-app"], |_| json_reply(view(false)));
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    assert_eq!(
        result.lines(),
        ["GET http://127.0.0.1:4317/repos/my-app/trust"]
    );
    assert!(result.stdout.contains("Commands of my-app: not approved"));
    assert!(result.stdout.contains("Stage test: check\n    pnpm test\n"));
    assert!(
        result
            .stdout
            .contains("natamux repo trust my-app --approve ab12")
    );
}

#[test]
fn approves_the_hash_given() {
    let result = run(&["repo", "trust", "my-app", "--approve", "ab12"], |_| {
        json_reply(view(true))
    });
    assert_eq!(result.code, EXIT_OK, "{}", result.stderr);
    let call = &result.calls[0];
    assert_eq!(call.method, "POST");
    assert_eq!(call.json().unwrap(), json!({ "hash": "ab12" }));
    assert_eq!(result.stdout, "Approved the commands of my-app.\n");
}

#[test]
fn says_why_an_old_hash_is_refused() {
    let refused = json!({ "error": "conflict", "message": "The commands changed." });
    let result = run(
        &["repo", "trust", "my-app", "--approve", "old"],
        move |_| json_status(409, refused.clone()),
    );
    assert_eq!(result.code, EXIT_FAILED);
    assert!(result.stderr.contains("The commands changed."));
}

#[test]
fn needs_trust_and_a_project_id() {
    for argv in [
        &["repo", "list"][..],
        &["repo", "trust"],
        &["repo", "trust", "Not An Id"],
    ] {
        let result = run(argv, |_| json_reply(view(true)));
        assert_eq!(result.code, EXIT_USAGE, "{argv:?}");
        assert!(result.calls.is_empty());
    }
}
