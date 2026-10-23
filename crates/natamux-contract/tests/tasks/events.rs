//! 15 `events` and 16 `delete`.

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon, Token};
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{agent_token, both, call, data_file, get, kept, post};

async fn send(d: &Daemon, id: &str, body: Value) -> Value {
    kept(&post(d, &format!("/tasks/{id}/events"), body).await)
}

fn human(kind: &str, payload: Value) -> Value {
    json!({ "source": "human", "type": kind, "payload": payload })
}

/// The contract pipeline: outline, a group (left, right), signoff, ship.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn events() {
    both("events", &[], |d: Daemon, mut t: Transcript| async move {
        let id = "CT-1501";
        post(&d, "/tasks", json!({ "title": format!("{id}: Walk the pipeline") })).await;
        for (step, body) in [
            ("readied", human("task.readied", json!({}))),
            ("started", human("stage.started", json!({}))),
            ("asked", json!({ "source": "agent", "type": "agent.asked", "payload": { "question": "Which?" } })),
            ("approved", human("stage.approved", json!({}))),
            ("left passed", human("stage.passed", json!({ "stage": "left" }))),
            ("right approved", human("stage.approved", json!({}))),
            ("changes asked", human("review.changes-requested", json!({}))),
            ("back to outline", human("stage.reopened", json!({ "to": "outline" }))),
            ("blocked by hand", human("state.overridden", json!({ "to": "blocked" }))),
            ("path is dropped", human("task.unblocked", json!({ "path": "x", "because": "y", "keep": 1 }))),
            ("cancelled", human("task.cancelled", json!({}))),
            ("after the end", human("task.readied", json!({}))),
            ("record only", human("task.created", json!({}))),
            ("unknown type", human("task.exploded", json!({}))),
            ("bad source", json!({ "source": "robot", "type": "task.readied" })),
            ("bad payload", json!({ "source": "human", "type": "task.readied", "payload": [1] })),
        ] {
            t.note(step, send(&d, id, body).await);
        }
        t.note("log", kept(&get(&d, &format!("/tasks/{id}/events")).await));
        t.note("unknown task", send(&d, "CT-99999", human("task.readied", json!({}))).await);

        let token = agent_token(&d).await;
        let agent = |body: Value| {
            let d = d.clone();
            let token = token.clone();
            async move {
                let path = "/tasks/CT-1502/events";
                kept(&d.request(Method::POST, path, Body::Json(body), Token::Given(&token)).await)
            }
        };
        post(&d, "/tasks", json!({ "title": "CT-1502: By an agent" })).await;
        t.note("agent as a person", agent(human("task.readied", json!({}))).await);
        for kind in ["review.approved", "stage.approved", "stage.passed", "stage.skipped", "state.overridden"] {
            t.note(&format!("agent sends {kind}"), agent(human(kind, json!({}))).await);
        }
        t.note("agent reads", kept(&d.request(Method::GET, "/tasks/CT-1502", Body::None, Token::Given(&token)).await));
        t.note("agent log", kept(&get(&d, "/tasks/CT-1502/events").await));
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn delete() {
    both("delete", &[], |d: Daemon, mut t: Transcript| async move {
        post(&d, "/tasks", json!({ "title": "CT-1601: Parent" })).await;
        // Apart in time, so the newest-first order of the freed tasks is the same on both.
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let child = json!({ "title": "CT-1602: Child", "parentId": "CT-1601" });
        post(&d, "/tasks", child).await;
        tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        let waits = json!({ "title": "CT-1603: Waits", "dependsOn": ["CT-1601", "CT-1602"] });
        post(&d, "/tasks", waits).await;
        t.note(
            "delete",
            kept(&call(&d, Method::DELETE, "/tasks/CT-1601", Body::None).await),
        );
        let left = d
            .exec(&format!(
                "ls '{}/tasks' | grep '^CT-1601' || true",
                d.data_dir()
            ))
            .await;
        t.note("files left", json!(left));
        t.note(
            "again",
            kept(&call(&d, Method::DELETE, "/tasks/CT-1601", Body::None).await),
        );
        t.note("child", kept(&get(&d, "/tasks/CT-1602").await));
        t.note("waiting", kept(&get(&d, "/tasks/CT-1603").await));
        t.note("child log", kept(&get(&d, "/tasks/CT-1602/events").await));
        t.note(
            "file of a child",
            json!(crate::clean_text(&data_file(&d, "tasks/CT-1602.md").await)),
        );
        t
    })
    .await;
}
