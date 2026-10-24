//! The `changes`, `files explorer` and `changed images` groups of `server.test.ts`.

use std::path::PathBuf;

use hyper::Method;
use serde_json::{Value, json};

use crate::git_repo::git_daemon;
use crate::server::{TOKEN, TestDaemon};

const PNG: [u8; 9] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0];

async fn with_workspace(name: &str) -> (TestDaemon, PathBuf) {
    let d = git_daemon(name).await;
    let workspace = d.task_with_workspace().await;
    (d, workspace)
}

async fn changes(d: &TestDaemon, query: &str) -> Value {
    d.get(&format!("/tasks/T-0001/changes{query}")).await.1["changes"].clone()
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_changed_files_and_gives_both_sides_of_one() {
    let (d, workspace) = with_workspace("changes-list").await;
    std::fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    let (status, list) = d.get("/tasks/T-0001/changes").await;

    assert_eq!(status, 200, "{list}");
    let expected = json!({
        "path": "notes.txt", "status": "U", "originalPath": null, "added": 1, "removed": 0,
    });
    assert_eq!(list["changes"], json!([expected]));
    let (status, file) = d.get("/tasks/T-0001/changes/file?path=notes.txt").await;
    assert_eq!(status, 200, "{file}");
    assert_eq!(file["path"], "notes.txt");
    assert!(file["original"].is_null());
    assert_eq!(file["modified"], "hello\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_list_of_a_timed_read_and_reads_git_now_with_fresh_1() {
    let (d, workspace) = with_workspace("changes-fresh").await;
    assert_eq!(changes(&d, "").await, json!([]));
    std::fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    assert_eq!(changes(&d, "").await, json!([]));
    assert_eq!(changes(&d, "?fresh=1").await.as_array().unwrap().len(), 1);
    assert_eq!(changes(&d, "").await.as_array().unwrap().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_a_file_that_is_not_a_changed_file() {
    let (d, _) = with_workspace("changes-404").await;

    let (status, _) = d
        .get("/tasks/T-0001/changes/file?path=..%2F..%2Fetc%2Fpasswd")
        .await;

    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_without_a_path() {
    let (d, _) = with_workspace("changes-400").await;

    assert_eq!(d.get("/tasks/T-0001/changes/file").await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_409_for_a_task_without_a_workspace_and_404_for_an_unknown_task() {
    let d = git_daemon("changes-409").await;
    d.make_task("Fix the bug").await;

    assert_eq!(d.get("/tasks/T-0001/changes").await.0, 409);
    assert_eq!(d.get("/tasks/T-0099/changes").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token() {
    let d = git_daemon("changes-token").await;

    let (status, _) = d.call(Method::GET, "/tasks/T-0001/changes", None).await;

    assert_eq!(status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_one_folder_of_the_worktree_without_git() {
    let (d, workspace) = with_workspace("tree-list").await;
    std::fs::create_dir(workspace.join("src")).unwrap();
    std::fs::write(workspace.join("src").join("a.ts"), "").unwrap();
    std::fs::write(workspace.join("notes.txt"), "").unwrap();

    let (status, root) = d.get("/tasks/T-0001/tree").await;

    assert_eq!(status, 200, "{root}");
    // The harness's first commit has a README.md; Node's test repository has no file.
    assert_eq!(
        root["entries"],
        json!([
            { "name": "src", "kind": "folder" },
            { "name": "notes.txt", "kind": "file" },
            { "name": "README.md", "kind": "file" }
        ])
    );
    let (_, src) = d.get("/tasks/T-0001/tree?path=src").await;
    assert_eq!(src["entries"], json!([{ "name": "a.ts", "kind": "file" }]));
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_folders_and_files_outside_the_worktree_or_inside_git() {
    let (d, _) = with_workspace("tree-refuse").await;

    assert_eq!(d.get("/tasks/T-0001/tree?path=..").await.0, 409);
    assert_eq!(d.get("/tasks/T-0001/tree?path=.git").await.0, 409);
    let outside = "/tasks/T-0001/file?path=..%2F..%2Fetc%2Fpasswd";
    assert_eq!(d.get(outside).await.0, 409);
    assert_eq!(d.get("/tasks/T-0001/tree?path=nope").await.0, 404);
    assert_eq!(d.get("/tasks/T-0001/file?path=nope.ts").await.0, 404);
    assert_eq!(d.get("/tasks/T-0001/file").await.0, 400);
}

/// `PUT /tasks/T-0001/file?path=<path>` with the headers and the text.
async fn save(d: &TestDaemon, path: &str, headers: &[(&str, &str)], text: &str) -> (u16, Value) {
    let url = format!("/tasks/T-0001/file?path={path}");
    let body = json!({ "text": text });
    let (status, _, bytes) = d
        .exchange_with(Method::PUT, &url, Some(TOKEN), headers, Some(&body))
        .await;
    (
        status,
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    )
}

fn version_of(read: &Value) -> String {
    read["version"].as_str().unwrap().to_string()
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_a_file_and_saves_it_with_the_version_it_read() {
    let (d, workspace) = with_workspace("file-save").await;
    let file = workspace.join("notes.txt");
    std::fs::write(&file, "hello\n").unwrap();

    let (status, read) = d.get("/tasks/T-0001/file?path=notes.txt").await;
    assert_eq!(status, 200, "{read}");
    assert_eq!(read["text"], "hello\n");
    let version = version_of(&read);

    let (status, saved) = save(&d, "notes.txt", &[("if-match", version.as_str())], "bye\n").await;
    assert_eq!(status, 200, "{saved}");
    assert_ne!(version_of(&saved), version);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "bye\n");

    // The editor still holds the first version: the file changed since, so it is not replaced.
    let (status, _) = save(&d, "notes.txt", &[("if-match", version.as_str())], "mine\n").await;
    assert_eq!(status, 409);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "bye\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_a_save_without_if_match_of_a_folder_or_over_1_mb() {
    let (d, workspace) = with_workspace("file-refuse").await;
    let file = workspace.join("notes.txt");
    std::fs::write(&file, "hello\n").unwrap();
    std::fs::create_dir(workspace.join("src")).unwrap();
    let version = version_of(&d.get("/tasks/T-0001/file?path=notes.txt").await.1);
    let with = [("if-match", version.as_str())];

    assert_eq!(save(&d, "notes.txt", &[], "x").await.0, 400);
    assert_eq!(save(&d, "src", &with, "x").await.0, 400);
    assert_eq!(save(&d, "new.ts", &with, "x").await.0, 404);
    let big = save(&d, "notes.txt", &with, &"a".repeat(1_000_001)).await;
    assert_eq!(big.0, 413);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "hello\n");
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_the_changes_again_after_a_save() {
    let (d, workspace) = with_workspace("file-changes").await;
    // The list is kept until the workspace may have changed.
    assert_eq!(changes(&d, "").await, json!([]));
    std::fs::write(workspace.join("notes.txt"), "hello\n").unwrap();
    let version = version_of(&d.get("/tasks/T-0001/file?path=notes.txt").await.1);

    let (status, _) = save(&d, "notes.txt", &[("if-match", version.as_str())], "bye\n").await;

    assert_eq!(status, 200);
    let after = changes(&d, "").await;
    assert_eq!(after[0]["path"], "notes.txt");
}

async fn with_image(name: &str) -> (TestDaemon, PathBuf) {
    let (d, workspace) = with_workspace(name).await;
    std::fs::write(workspace.join("logo.png"), PNG).unwrap();
    (d, workspace)
}

async fn image(d: &TestDaemon, query: &str) -> (u16, hyper::HeaderMap, Vec<u8>) {
    let url = format!("/tasks/T-0001/changes/image?{query}");
    d.exchange(Method::GET, &url, Some(TOKEN), None).await
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_the_bytes_with_a_type_nosniff_and_a_policy_that_runs_nothing() {
    let (d, _) = with_image("image-bytes").await;

    let (status, headers, bytes) = image(&d, "path=logo.png&side=modified").await;

    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/png");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["cache-control"], "no-store");
    assert_eq!(
        headers["content-security-policy"],
        "default-src 'none'; style-src 'unsafe-inline'; sandbox"
    );
    assert_eq!(bytes, PNG);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_400_for_an_unknown_side_or_a_file_that_is_not_an_image() {
    let (d, workspace) = with_image("image-400").await;
    std::fs::write(workspace.join("notes.txt"), "hello\n").unwrap();

    assert_eq!(image(&d, "path=logo.png&side=both").await.0, 400);
    assert_eq!(image(&d, "path=notes.txt&side=modified").await.0, 400);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_a_side_that_is_not_there_or_a_file_that_did_not_change() {
    let (d, _) = with_image("image-404").await;

    assert_eq!(image(&d, "path=logo.png&side=original").await.0, 404);
    assert_eq!(image(&d, "path=other.png&side=modified").await.0, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_413_for_an_image_that_is_too_large() {
    let (d, workspace) = with_image("image-413").await;
    std::fs::write(workspace.join("big.png"), vec![0u8; 5_000_001]).unwrap();

    let (status, _, bytes) = image(&d, "path=big.png&side=modified").await;

    assert_eq!(status, 413);
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(body["error"], "too_large");
}
