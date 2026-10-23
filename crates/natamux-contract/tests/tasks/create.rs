//! 13 `create_and_read` and 14 `edit`.

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon};
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{both, call, get, kept, post};

/// Our tasks of a list, so tasks of other scenarios do not count.
fn ours(answer: &natamux_contract::daemon::Answer, prefix: &str) -> Value {
    let tasks: Vec<Value> = answer.body["tasks"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|task| task["id"].as_str().is_some_and(|id| id.starts_with(prefix)))
        .cloned()
        .collect();
    json!([answer.status, crate::clean(json!(tasks))])
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn create_and_read() {
    both("create_and_read", &[], |d: Daemon, mut t: Transcript| async move {
        t.note("plain", kept(&post(&d, "/tasks", json!({ "title": "CT-1301: First task" })).await));
        let full = json!({
            "title": "  CT-1302 Second, with details  ", "description": "Line one.\n\nLine three.",
            "priority": "high", "labels": ["bug", "ui"], "dependsOn": ["CT-1301"],
            "context": { "source": "manual", "ref": " #12 " }, "extra": "dropped",
        });
        t.note("full", kept(&post(&d, "/tasks", full).await));
        let sub = json!({ "title": "CT-1303: A subtask", "parentId": "CT-1302" });
        t.note("subtask", kept(&post(&d, "/tasks", sub).await));
        t.note("get", kept(&get(&d, "/tasks/CT-1302").await));
        t.note("list", ours(&get(&d, "/tasks").await, "CT-13"));
        t.note("list draft", ours(&get(&d, "/tasks?state=draft").await, "CT-13"));
        t.note("bad state", kept(&get(&d, "/tasks?state=nope").await));
        t.note("two states", kept(&get(&d, "/tasks?state=draft&state=ready").await));
        t.note("used id", kept(&post(&d, "/tasks", json!({ "title": "CT-1301: again" })).await));
        let bad = json!({ "title": "", "priority": "top", "labels": [""], "dependsOn": ["x"], "agent": "" });
        t.note("bad body", kept(&post(&d, "/tasks", bad).await));
        let long = "x".repeat(401);
        t.note("long title", kept(&post(&d, "/tasks", json!({ "title": long })).await));
        t.note("no body", kept(&call(&d, Method::POST, "/tasks", Body::None).await));
        let empty = Body::Raw(Some("application/json"), Vec::new());
        t.note("empty JSON body", kept(&call(&d, Method::POST, "/tasks", empty).await));
        let broken = Body::Raw(Some("application/json"), b"{\"title\":".to_vec());
        let answer = call(&d, Method::POST, "/tasks", broken).await;
        t.note("invalid JSON status", json!([answer.status, answer.body["error"]]));
        t.note("invalid JSON", kept(&answer));
        let text = Body::Raw(Some("text/plain"), b"CT-1304: text".to_vec());
        t.note("text body", kept(&call(&d, Method::POST, "/tasks", text).await));
        let html = Body::Raw(Some("text/html"), b"<p>".to_vec());
        t.note("html body", kept(&call(&d, Method::POST, "/tasks", html).await));
        let proto = Body::Raw(Some("application/json"), br#"{"title":"CT-1305: x","__proto__":{"a":1}}"#.to_vec());
        t.note("prototype key", kept(&call(&d, Method::POST, "/tasks", proto).await));
        t.note("unknown repo", kept(&post(&d, "/tasks", json!({ "title": "CT-1306: x", "repo": "nope" })).await));
        t.note("bad repo id", kept(&post(&d, "/tasks", json!({ "title": "CT-1307: x", "repo": "No" })).await));
        t.note("unknown task", kept(&get(&d, "/tasks/CT-99999").await));
        t.note("bad task id", kept(&get(&d, "/tasks/nope").await));
        t.note("long task id", kept(&get(&d, &format!("/tasks/T-{}", "1".repeat(40))).await));
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn edit() {
    both("edit", &[], |d: Daemon, mut t: Transcript| async move {
        post(&d, "/tasks", json!({ "title": "CT-1401: Parent" })).await;
        post(
            &d,
            "/tasks",
            json!({ "title": "CT-1402: Child", "parentId": "CT-1401" }),
        )
        .await;
        post(
            &d,
            "/tasks",
            json!({ "title": "CT-1403: Other", "dependsOn": ["CT-1402"] }),
        )
        .await;
        let patch = |body: Value| {
            let d = d.clone();
            async move { kept(&call(&d, Method::PATCH, "/tasks/CT-1402", Body::Json(body)).await) }
        };
        t.note(
            "change",
            patch(json!({ "title": " New title ", "labels": [" a "], "priority": "low" })).await,
        );
        t.note("no change", patch(json!({ "priority": "low" })).await);
        t.note(
            "unknown key",
            patch(json!({ "state": "done", "x": 1 })).await,
        );
        t.note(
            "unknown dependency",
            patch(json!({ "dependsOn": ["CT-9999"] })).await,
        );
        t.note("loop", patch(json!({ "dependsOn": ["CT-1403"] })).await);
        t.note(
            "on its parent",
            patch(json!({ "dependsOn": ["CT-1401"] })).await,
        );
        t.note(
            "bad values",
            patch(json!({ "title": 3, "labels": "a", "description": null })).await,
        );
        t.note(
            "events",
            kept(&crate::get(&d, "/tasks/CT-1402/events").await),
        );
        let missing = call(&d, Method::PATCH, "/tasks/CT-99999", Body::Json(json!({}))).await;
        t.note("unknown task", kept(&missing));
        t
    })
    .await;
}
