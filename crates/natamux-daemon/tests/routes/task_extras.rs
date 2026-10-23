//! The `attachments` group and the `pushed to the dashboards` group of `server.test.ts`. A new
//! task brings the files, as a new workspace does in Node.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use hyper::Method;
use natamux_daemon::shared::event_bus::DaemonMessage;
use serde_json::{Value, json};

use crate::server::{AGENT_TOKEN, TOKEN, TestDaemon, bare, eventually};

const PNG: [u8; 9] = [0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0];

fn file(name: &str, bytes: &[u8]) -> Value {
    json!({ "name": name, "data": STANDARD.encode(bytes) })
}

async fn new_task(d: &TestDaemon, files: Vec<Value>) -> u16 {
    let body = json!({ "title": "Fix the login", "attachments": files });
    d.send(Method::POST, "/tasks", Some(TOKEN), &body).await.0
}

async fn attachment(d: &TestDaemon, name: &str) -> (u16, hyper::HeaderMap, Vec<u8>) {
    let path = format!("/tasks/T-0001/attachments/{}", name.replace(' ', "%20"));
    d.exchange(Method::GET, &path, Some(TOKEN), None).await
}

#[tokio::test(flavor = "multi_thread")]
async fn keeps_the_files_sent_with_a_new_task_and_lists_them() {
    let d = bare("att-list").await;
    let files = vec![
        file("login error.png", &PNG),
        file("spec.docx", b"word"),
        file("spec.docx", b"word again"),
    ];
    assert_eq!(new_task(&d, files).await, 201);

    let (_, listed) = d
        .call(Method::GET, "/tasks/T-0001/attachments", Some(TOKEN))
        .await;
    assert_eq!(
        listed["attachments"],
        json!([
            { "name": "login-error.png", "size": PNG.len(), "type": "image/png" },
            { "name": "spec-2.docx", "size": 10, "type": null },
            { "name": "spec.docx", "size": 4, "type": null },
        ])
    );
    let (_, events) = d
        .call(Method::GET, "/tasks/T-0001/events", Some(TOKEN))
        .await;
    let added = events["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|entry| entry["event"]["type"] == "attachment.added")
        .count();
    assert_eq!(added, 3);
}

#[tokio::test(flavor = "multi_thread")]
async fn reads_back_a_file_sent_with_a_new_task() {
    let d = bare("att-read").await;
    assert_eq!(new_task(&d, vec![file("notes.md", b"# Notes")]).await, 201);
    assert_eq!(attachment(&d, "notes.md").await.2, b"# Notes");
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_an_image_to_show_with_nosniff_and_a_policy_that_runs_nothing() {
    let d = bare("att-image").await;
    new_task(&d, vec![file("shot.png", &PNG)]).await;

    let (status, headers, bytes) = attachment(&d, "shot.png").await;

    assert_eq!(status, 200);
    assert_eq!(headers["content-type"], "image/png");
    assert!(headers.get("content-disposition").is_none());
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert!(
        headers["content-security-policy"]
            .to_str()
            .unwrap()
            .contains("sandbox")
    );
    assert_eq!(bytes, PNG);
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_a_page_a_logo_and_a_pdf_only_as_a_download() {
    for name in ["page.html", "logo.svg", "report.pdf"] {
        let d = bare("att-download").await;
        new_task(&d, vec![file(name, b"<script>alert(1)</script>")]).await;

        let (status, headers, _) = attachment(&d, name).await;

        assert_eq!(status, 200, "{name}");
        assert_eq!(
            headers["content-type"], "application/octet-stream",
            "{name}"
        );
        assert_eq!(
            headers["content-disposition"],
            format!("attachment; filename=\"{name}\"").as_str(),
            "{name}"
        );
        assert_eq!(headers["x-content-type-options"], "nosniff", "{name}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_404_for_a_file_the_task_lacks_and_400_for_a_name_that_is_not_safe() {
    let d = bare("att-errors").await;
    new_task(&d, vec![file("a.txt", b"a")]).await;

    assert_eq!(attachment(&d, "b.txt").await.0, 404);
    assert_eq!(attachment(&d, ".hidden").await.0, 400);
    assert_eq!(attachment(&d, "a b.txt").await.0, 400);
    let (status, _) = d
        .call(Method::GET, "/tasks/T-0042/attachments", Some(TOKEN))
        .await;
    assert_eq!(status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn refuses_more_than_20_files_and_makes_no_task() {
    let d = bare("att-many").await;
    let files = (0..21).map(|n| file(&format!("f{n}.txt"), b"x")).collect();

    assert_eq!(new_task(&d, files).await, 400);

    let (_, tasks) = d.call(Method::GET, "/tasks", Some(TOKEN)).await;
    assert_eq!(tasks["tasks"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_a_body_larger_than_1_mb_when_it_brings_files() {
    let d = bare("att-big").await;

    assert_eq!(
        new_task(&d, vec![file("big.bin", &vec![0; 2_000_000])]).await,
        201
    );

    assert_eq!(attachment(&d, "big.bin").await.2.len(), 2_000_000);
}

fn shells_changed() -> (DaemonMessage, Value) {
    (
        DaemonMessage::ShellsChanged {
            task_id: "T-0001".into(),
        },
        json!({ "kind": "shells.changed", "taskId": "T-0001" }),
    )
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_each_message_to_every_dashboard() {
    let d = bare("dash-all").await;
    let mut dashboards = vec![
        d.dashboard(TOKEN).await,
        d.dashboard(TOKEN).await,
        d.dashboard(TOKEN).await,
    ];
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 3).await;
    let (message, text) = shells_changed();

    bus.publish(message);

    for dashboard in &mut dashboards {
        dashboard
            .until(|socket| !socket.controls().is_empty())
            .await;
        assert_eq!(dashboard.controls(), std::slice::from_ref(&text));
    }
    for dashboard in dashboards {
        dashboard.drop_connection().await;
    }
    eventually(|| bus.size() == 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_the_messages_to_the_agent_token_of_a_task_terminal_too() {
    let d = bare("dash-agent").await;
    let mut dashboard = d.dashboard(AGENT_TOKEN).await;
    let bus = std::sync::Arc::clone(&d.daemon.state.bus);
    eventually(|| bus.size() == 1).await;
    let (message, text) = shells_changed();

    bus.publish(message);

    dashboard
        .until(|socket| !socket.controls().is_empty())
        .await;
    assert_eq!(dashboard.controls(), [text]);
    dashboard.drop_connection().await;
    eventually(|| bus.size() == 0).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn tells_a_dashboard_about_a_task_made_over_http() {
    let d = bare("dash-task").await;
    let mut dashboard = d.dashboard(TOKEN).await;
    eventually(|| d.daemon.state.bus.size() == 1).await;

    new_task(&d, vec![]).await;

    dashboard
        .until(|socket| {
            socket
                .controls()
                .iter()
                .any(|message| message["kind"] == "task.created")
        })
        .await;
}
