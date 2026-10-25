//! The routes of the Changes and History tabs: method, path, query, body and answer, and the
//! errors of the image route.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use natamux_client::{ClientError, FeedTimes};
use natamux_proto::workspaces::changes::{ChangeStatus, DraftSource, ImageSide};
use serde_json::json;
use support::{exchange, ok};

const TASK: &str =
    r#"{"id":"T-0009","title":"Fix it","state":"draft","updatedAt":"2026-10-22T00:00:00.000Z"}"#;

#[test]
fn the_two_reads_ask_for_fresh_data_only_when_told() {
    let changes = r#"{"changes":[{"path":"a.ts","status":"U","added":1,"removed":0}]}"#;
    let (list, _) = ok("GET /tasks/T-0009/changes?fresh=1", changes, |client| {
        client.list_changes("T-0009", true)
    });
    assert_eq!(list[0].status, ChangeStatus::Untracked);
    let graph = r#"{"head":"abc","commits":[{"hash":"abc","parents":["def"],"author":"A","date":"2026-10-22T00:00:00Z","subject":"Fix","refs":["HEAD"],"onBranch":true}],"truncated":false}"#;
    let (graph, _) = ok("GET /tasks/T-0009/graph?fresh=1", graph, |client| {
        client.graph("T-0009", true)
    });
    assert_eq!(graph.commits[0].parents, ["def"]);
    assert!(graph.commits[0].on_branch);
    let (_, _) = ok("GET /tasks/T-0009/graph", r#"{"head":"abc"}"#, |client| {
        client.graph("T-0009", false)
    });
}

#[test]
fn reads_the_diff_of_a_file_with_its_path_encoded() {
    let body = r#"{"path":"src/a b.ts","status":"M","original":"old","modified":"new","binary":false,"tooLarge":false}"#;
    let (diff, sent) = ok(
        "GET /tasks/T-0009/changes/file?path=src%2Fa%20b.ts",
        body,
        |client| client.file_diff("T-0009", "src/a b.ts"),
    );
    assert_eq!(diff.original.as_deref(), Some("old"));
    assert_eq!(diff.modified.as_deref(), Some("new"));
    assert_eq!(sent, serde_json::Value::Null);
}

#[test]
fn reads_the_bytes_of_an_image_unchanged() {
    let bytes = "\u{89}PNG\r\n\u{1a}\n\u{0}\u{7f}é";
    for (side, name) in [
        (ImageSide::Original, "original"),
        (ImageSide::Modified, "modified"),
    ] {
        let route = format!("GET /tasks/T-0009/changes/image?path=a.png&side={name}");
        let (got, _) = ok(&route, bytes, |client| {
            client.file_image("T-0009", "a.png", side)
        });
        assert_eq!(got, bytes.as_bytes());
    }
}

#[test]
fn an_image_that_is_too_large_or_missing_is_a_status_error() {
    let route = "GET /tasks/T-0009/changes/image?path=a.png&side=modified";
    for (status, line, message) in [
        (413, "413 Payload Too Large", "Image is larger than 5 MB"),
        (404, "404 Not Found", "No such side"),
    ] {
        let body = format!(r#"{{"message":"{message}"}}"#);
        let (answer, _) = exchange(route, line, &body, |client| {
            client.file_image("T-0009", "a.png", ImageSide::Modified)
        });
        assert_eq!(
            answer.unwrap_err(),
            ClientError::Status {
                status,
                message: message.into()
            }
        );
    }
}

#[test]
fn commits_with_the_message() {
    let body = format!(r#"{{"task":{TASK},"hash":"abc123"}}"#);
    let (done, sent) = ok("POST /tasks/T-0009/commit", &body, |client| {
        client.commit("T-0009", "Fix it".into())
    });
    assert_eq!(done.hash, "abc123");
    assert_eq!(done.task.id, "T-0009");
    assert_eq!(sent, json!({"message": "Fix it"}));
}

#[test]
fn asks_for_a_commit_message_with_no_body() {
    let body = r#"{"message":"Fix it","source":"agent","agent":"claude-code"}"#;
    let (draft, sent) = ok("POST /tasks/T-0009/commit-message", body, |client| {
        client.commit_message("T-0009")
    });
    assert_eq!(draft.source, DraftSource::Agent);
    assert_eq!(draft.agent, "claude-code");
    assert_eq!(sent, serde_json::Value::Null);
    let body = r#"{"message":"Update a.ts","source":"draft","agent":"","reason":"no agent"}"#;
    let (draft, _) = ok("POST /tasks/T-0009/commit-message", body, |client| {
        client.commit_message("T-0009")
    });
    assert_eq!(draft.source, DraftSource::Draft);
    assert_eq!(draft.reason.as_deref(), Some("no agent"));
}

#[test]
fn rolls_back_the_paths_and_undoes_it_with_no_body() {
    let (result, sent) = ok("POST /tasks/T-0009/rollback", r#"{"files":2}"#, |client| {
        client.rollback("T-0009", vec!["a.ts".into(), "b/c.ts".into()])
    });
    assert_eq!(result.files, 2);
    assert_eq!(sent, json!({"paths": ["a.ts", "b/c.ts"]}));
    let (result, sent) = ok(
        "POST /tasks/T-0009/rollback/undo",
        r#"{"files":2}"#,
        |client| client.undo_rollback("T-0009"),
    );
    assert_eq!(result.files, 2);
    assert_eq!(sent, serde_json::Value::Null);
}

#[test]
fn sends_text_to_the_agent() {
    let body = format!(r#"{{"task":{TASK}}}"#);
    let (task, sent) = ok("POST /tasks/T-0009/agent/message", &body, |client| {
        client.send_to_agent("T-0009", "Use a map".into())
    });
    assert_eq!(task.id, "T-0009");
    assert_eq!(sent, json!({"text": "Use a map"}));
}

#[test]
fn a_refused_write_keeps_its_status_and_message() {
    let (answer, _) = exchange(
        "POST /tasks/T-0009/commit",
        "409 Conflict",
        r#"{"message":"Nothing to commit"}"#,
        |client| client.commit("T-0009", "x".into()),
    );
    assert_eq!(
        answer.unwrap_err(),
        ClientError::Status {
            status: 409,
            message: "Nothing to commit".into()
        }
    );
}

// The web dashboard reads the diff sizes at most this often after `workspace.dirty`.
#[test]
fn the_diff_stats_time_matches_the_web_dashboard() {
    let source = include_str!("../../../packages/web/src/lib/live.ts");
    let marker = "export const DIFFSTATS_EVERY_MS = ";
    let start = source.find(marker).unwrap() + marker.len();
    let millis: u64 = source[start..]
        .lines()
        .next()
        .unwrap()
        .replace('_', "")
        .parse()
        .unwrap();
    assert_eq!(
        FeedTimes::default().diff_stats_every,
        Duration::from_millis(millis)
    );
}
