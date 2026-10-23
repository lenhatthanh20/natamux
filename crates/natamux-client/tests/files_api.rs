//! The routes of the Files tab, the editor tab and the Docs tab: method, path, query, the
//! `If-Match` header, body, answer, and the refusals the screens tell apart.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_client::ClientError;
use natamux_proto::workspaces::files::{EntryKind, SavedFile};
use serde_json::{Value, json};
use support::{FakeDaemon, exchange, ok, runtime, within};

const FILE: &str =
    r#"{"path":"a b/c.ts","text":"x","binary":false,"tooLarge":false,"version":"v1"}"#;

fn status_error(answer: Result<impl std::fmt::Debug, ClientError>) -> (u16, String) {
    match answer {
        Err(ClientError::Status { status, message }) => (status, message),
        other => panic!("expected a status error, got {other:?}"),
    }
}

#[test]
fn lists_one_folder_with_its_path_encoded() {
    let body = r#"{"entries":[{"name":"src","kind":"folder"},{"name":"a.ts","kind":"file"}]}"#;
    let (entries, sent) = ok(
        "GET /tasks/T-1/tree?path=packages%2Fweb%20src",
        body,
        |client| client.list_folder("T-1", "packages/web src"),
    );
    assert_eq!(entries[0].kind, EntryKind::Folder);
    assert_eq!(entries[1].name, "a.ts");
    assert_eq!(sent, Value::Null);
    let (root, _) = ok("GET /tasks/T-1/tree?path=", r#"{"entries":[]}"#, |client| {
        client.list_folder("T-1", "")
    });
    assert!(root.is_empty());
}

#[test]
fn reads_a_file_of_the_worktree_and_saves_it_with_the_version_it_read() {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    let route = "/tasks/T-1/file?path=a%20b%2Fc.ts";
    daemon.state(|state| {
        state.answer(&format!("GET {route}"), "200 OK", FILE);
        state.answer(
            &format!("PUT {route}"),
            "200 OK",
            r#"{"path":"a b/c.ts","version":"v2"}"#,
        );
    });
    let client = daemon.client();
    let file = rt
        .block_on(within(client.read_workspace_file("T-1", "a b/c.ts")))
        .unwrap();
    assert_eq!(
        (file.text.as_deref(), file.version.as_str()),
        (Some("x"), "v1")
    );
    let saved = rt
        .block_on(within(client.write_workspace_file(
            "T-1",
            "a b/c.ts",
            "y".into(),
            file.version,
        )))
        .unwrap();
    assert_eq!(
        saved,
        SavedFile {
            path: "a b/c.ts".into(),
            version: "v2".into()
        }
    );
    daemon.state(|state| {
        let (seen, sent) = state.last_request();
        assert_eq!(seen, format!("PUT {route}"));
        assert_eq!(
            serde_json::from_str::<Value>(&sent).unwrap(),
            json!({"text": "y"})
        );
        assert_eq!(state.last_header("if-match").as_deref(), Some("v1"));
        assert_eq!(
            state.last_header("content-type").as_deref(),
            Some("application/json")
        );
    });
}

#[test]
fn tells_a_file_that_changed_on_disk_from_other_failures() {
    let route = "PUT /tasks/T-1/file?path=a.ts";
    let (answer, _) = exchange(
        route,
        "409 Conflict",
        r#"{"error":"conflict","message":"a.ts changed on disk."}"#,
        |client| client.write_workspace_file("T-1", "a.ts", "y".into(), "old".into()),
    );
    let error = answer.unwrap_err();
    assert!(error.is_conflict());
    assert_eq!(
        error,
        ClientError::Status {
            status: 409,
            message: "a.ts changed on disk.".into()
        }
    );
    let (answer, _) = exchange(
        route,
        "413 Payload Too Large",
        r#"{"message":"a.ts is too large to edit"}"#,
        |client| client.write_workspace_file("T-1", "a.ts", "y".into(), "old".into()),
    );
    let error = answer.unwrap_err();
    assert!(!error.is_conflict());
    assert_eq!(status_error(Err::<(), _>(error)).0, 413);
}

#[test]
fn a_path_outside_the_worktree_and_a_missing_file_keep_the_daemon_message() {
    let (answer, _) = exchange(
        "GET /tasks/T-1/file?path=..%2Fx",
        "409 Conflict",
        r#"{"message":"../x points outside the workspace, so it is not shown"}"#,
        |client| client.read_workspace_file("T-1", "../x"),
    );
    assert_eq!(
        status_error(answer),
        (
            409,
            "../x points outside the workspace, so it is not shown".into()
        )
    );
    let (answer, _) = exchange(
        "GET /tasks/T-1/file?path=gone.ts",
        "404 Not Found",
        r#"{"message":"gone.ts not found"}"#,
        |client| client.read_workspace_file("T-1", "gone.ts"),
    );
    assert_eq!(status_error(answer), (404, "gone.ts not found".into()));
}

#[test]
fn reads_and_writes_a_stage_file_with_its_name_encoded() {
    let (file, sent) = ok(
        "GET /tasks/T-1/files/my%20spec.md",
        r##"{"name":"my spec.md","text":"# Spec\n"}"##,
        |client| client.read_stage_file("T-1", "my spec.md"),
    );
    assert_eq!(
        (file.name.as_str(), file.text.as_str()),
        ("my spec.md", "# Spec\n")
    );
    assert_eq!(sent, Value::Null);
    let (file, sent) = ok(
        "PUT /tasks/T-1/files/spec.md",
        r#"{"name":"spec.md","text":"new"}"#,
        |client| client.write_stage_file("T-1", "spec.md", "new".into()),
    );
    assert_eq!(file.text, "new");
    assert_eq!(sent, json!({"text": "new"}));
    let (answer, _) = exchange(
        "GET /tasks/T-1/files/none.md",
        "404 Not Found",
        r#"{"error":"not_found","message":"T-1 has no none.md"}"#,
        |client| client.read_stage_file("T-1", "none.md"),
    );
    assert_eq!(status_error(answer), (404, "T-1 has no none.md".into()));
}

#[test]
fn lists_the_attachments_and_reads_the_bytes_of_one() {
    let body = r#"{"attachments":[{"name":"screen.png","size":3,"type":"image/png"},{"name":"n.pdf","size":9,"type":null}]}"#;
    let (list, _) = ok("GET /tasks/T-1/attachments", body, |client| {
        client.list_attachments("T-1")
    });
    assert_eq!(list[0].kind.as_deref(), Some("image/png"));
    assert_eq!((list[1].size, list[1].kind.clone()), (9, None));
    let bytes = "\u{89}PNG\r\n\u{1a}\n\u{0}\u{7f}é";
    let (got, _) = ok("GET /tasks/T-1/attachments/a%20b.png", bytes, |client| {
        client.read_attachment("T-1", "a b.png")
    });
    assert_eq!(got, bytes.as_bytes());
    let (answer, _) = exchange(
        "GET /tasks/T-1/attachments/x.png",
        "404 Not Found",
        r#"{"message":"T-1 has no x.png"}"#,
        |client| client.read_attachment("T-1", "x.png"),
    );
    assert_eq!(status_error(answer), (404, "T-1 has no x.png".into()));
}
