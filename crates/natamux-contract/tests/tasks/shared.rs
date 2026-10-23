//! 22 `shared_data_folder`: what one daemon wrote, the other reads the same; and the files both
//! write for the same requests are the same bytes, but for the times.

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon, daemons};
use natamux_contract::transcript::{Transcript, compare};
use serde_json::{Value, json};

use crate::{both, call, clean_text, data_file, get, kept, post};

/// A task with a bit of everything, made the same way on either daemon.
async fn make(d: &Daemon, id: &str) {
    let png = "iVBORw0KGgo=";
    let task = json!({
        "title": format!("{id}: Shared, with 'quotes' and: colons"),
        "description": "Notes over\ntwo lines.", "priority": "urgent", "labels": ["a", "b c"],
        "context": { "source": "manual", "ref": "#1" },
        "attachments": [{ "name": "shot.png", "data": png }],
    });
    post(d, "/tasks", task).await;
    let events = format!("/tasks/{id}/events");
    post(
        d,
        &events,
        json!({ "source": "human", "type": "task.readied" }),
    )
    .await;
    post(
        d,
        &events,
        json!({ "source": "human", "type": "stage.started" }),
    )
    .await;
    let patch = Body::Json(json!({ "description": "Notes, edited." }));
    call(d, Method::PATCH, &format!("/tasks/{id}"), patch).await;
}

/// What a dashboard reads of a task.
async fn reads(d: &Daemon, id: &str) -> Value {
    json!({
        "task": kept(&get(d, &format!("/tasks/{id}")).await),
        "events": kept(&get(d, &format!("/tasks/{id}/events")).await),
        "attachments": kept(&get(d, &format!("/tasks/{id}/attachments")).await),
    })
}

/// Copies the files of a task from one data folder to the other.
async fn copy(d: &Daemon, from: &str, to: &str, id: &str) {
    d.exec(&format!(
        "mkdir -p '{to}/tasks' && cp '{from}/tasks/{id}.md' '{from}/tasks/{id}.events.jsonl' '{to}/tasks/' && cp -R '{from}/tasks/{id}' '{to}/tasks/'"
    ))
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn shared_data_folder() {
    let (node, rust) = daemons();
    let (node_data, rust_data) = (node.data_dir(), rust.data_dir());
    let (mut left, mut right) = (Transcript::new("node"), Transcript::new("rust"));

    // Node writes, Rust reads after a restart.
    make(&node, "CT-2201").await;
    left.note("node to rust", reads(&node, "CT-2201").await);
    copy(&node, &node_data, &rust_data, "CT-2201").await;
    rust.restart().await;
    right.note("node to rust", reads(&rust, "CT-2201").await);

    // Rust writes, Node reads after a restart (Node reads the event logs at its start only).
    make(&rust, "CT-2202").await;
    right.note("rust to node", reads(&rust, "CT-2202").await);
    copy(&rust, &rust_data, &node_data, "CT-2202").await;
    node.restart().await;
    left.note("rust to node", reads(&node, "CT-2202").await);
    compare("shared_data_folder", &left, &right, &[]);

    // The same requests on both: the same task file and event log, but for the times.
    both(
        "same_files",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            make(&d, "CT-2203").await;
            t.note(
                "task file",
                json!(clean_text(&data_file(&d, "tasks/CT-2203.md").await)),
            );
            t.note(
                "event log",
                json!(clean_text(
                    &data_file(&d, "tasks/CT-2203.events.jsonl").await
                )),
            );
            t
        },
    )
    .await;
}
