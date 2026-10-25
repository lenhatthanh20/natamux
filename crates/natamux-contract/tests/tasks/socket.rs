//! 20 `events_socket` and 21 `resync`.

use std::time::Duration;

use hyper::Method;
use natamux_contract::daemon::{Body, Daemon};
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{both, call, clean, post};

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn events_socket() {
    both(
        "events_socket",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let refused = Events::open(&d, None).await;
            t.note("no token", json!(refused.refused));
            let wrong = Events::open(&d, Some("wrong")).await;
            t.note("wrong token", json!(wrong.refused));
            let mut events = Events::open(&d, Some(&d.token)).await;
            t.note("open", json!(events.refused));
            let id = "CT-2001";
            post(&d, "/tasks", json!({ "title": format!("{id}: Watched") })).await;
            let patch = Body::Json(json!({ "labels": ["seen"] }));
            call(&d, Method::PATCH, &format!("/tasks/{id}"), patch).await;
            post(
                &d,
                &format!("/tasks/{id}/events"),
                json!({ "source": "human", "type": "task.readied" }),
            )
            .await;
            post(
                &d,
                &format!("/tasks/{id}/events"),
                json!({ "source": "human", "type": "task.retried" }),
            )
            .await;
            call(&d, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
            let ids = [
                "CT-2001", "CT-2002", "CT-2003", "CT-2004", "CT-2005", "CT-2006",
            ];
            more_requests(&d).await;
            let done = events
                .until(|messages| {
                    messages.iter().any(|message| {
                        message["kind"] == "task.deleted" && message["taskId"] == "CT-2004"
                    })
                })
                .await;
            t.note("all came", json!(done));
            t.note("messages", clean(Value::Array(events.about(&ids))));
            t
        },
    )
    .await;
}

/// The kinds of request of scenarios 13 to 19 that send messages: a task from a template, a task
/// with an attachment, an issue link, a change of nothing, and a delete that sets its subtask and
/// a task that waited for it free (`task.updated` of `detach`).
async fn more_requests(d: &Daemon) {
    let template =
        json!({ "name": "Socket template", "title": "From the socket", "labels": ["t"] });
    post(d, "/templates", template).await;
    post(
        d,
        "/templates/socket-template/tasks",
        json!({ "title": "CT-2002: From a template" }),
    )
    .await;
    let file = json!({ "name": "a.png", "data": "iVBORw0KGgo=" });
    post(
        d,
        "/tasks",
        json!({ "title": "CT-2003: With a file", "attachments": [file] }),
    )
    .await;
    let link =
        json!({ "provider": "github", "key": "#1", "url": "https://github.com/o/r/issues/1" });
    call(d, Method::PUT, "/tasks/CT-2002/issue", Body::Json(link)).await;
    call(
        d,
        Method::PATCH,
        "/tasks/CT-2003",
        Body::Json(json!({ "priority": "normal" })),
    )
    .await;
    post(d, "/tasks", json!({ "title": "CT-2004: Parent" })).await;
    // A delete sets its dependents free newest first; a fast daemon makes two tasks in one
    // millisecond, whose order is then the id's.
    tokio::time::sleep(Duration::from_millis(5)).await;
    let child = json!({ "title": "CT-2005: Child", "parentId": "CT-2004" });
    post(d, "/tasks", child).await;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let waits = json!({ "title": "CT-2006: Waits", "dependsOn": ["CT-2004"] });
    post(d, "/tasks", waits).await;
    call(d, Method::DELETE, "/tasks/CT-2004", Body::None).await;
}

/// A dashboard that stops reading while big messages come: it misses some, gets a `resync` when
/// it catches up (never two in a row), then live messages again. How many tasks it takes to fall
/// behind depends on the buffers (nginx stands before the Node daemon), so the flood goes in
/// batches of 30 until the first `resync` comes, and only the shape is compared.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn resync() {
    both("resync", &[], |d: Daemon, mut t: Transcript| async move {
        let mut events = Events::open(&d, Some(&d.token)).await;
        let notes = "x".repeat(400_000);
        let mut posted = 0_usize;
        let mut resynced = false;
        while !resynced && posted < 150 {
            for _ in 0..30 {
                posted += 1;
                let title = format!("CT-22{posted:03}: Big");
                post(
                    &d,
                    "/tasks",
                    json!({ "title": title, "description": notes }),
                )
                .await;
            }
            resynced = events
                .until_within(
                    |messages| messages.iter().any(|message| message["kind"] == "resync"),
                    Duration::from_secs(5),
                )
                .await;
        }
        assert!(resynced, "{}: no resync after {posted} big tasks", d.name);
        // The backlog can still bring a second resync, which would drop the task posted next.
        let mut seen = events.messages.len();
        for _ in 0..40 {
            events.read_for(Duration::from_millis(500)).await;
            if events.messages.len() == seen {
                break;
            }
            seen = events.messages.len();
        }
        post(&d, "/tasks", json!({ "title": "CT-2131: After" })).await;
        let live = events
            .until(|messages| {
                messages
                    .iter()
                    .any(|message| message["task"]["id"] == "CT-2131")
            })
            .await;
        let kinds: Vec<String> = events
            .messages
            .iter()
            .map(|message| match message["task"]["id"].as_str() {
                Some(id) => format!("{} {id}", message["kind"].as_str().unwrap_or("")),
                None => message["kind"].as_str().unwrap_or("").to_string(),
            })
            .collect();
        let resyncs: Vec<usize> = kinds
            .iter()
            .enumerate()
            .filter(|(_, kind)| *kind == "resync")
            .map(|(at, _)| at)
            .collect();
        // "task.created " is 13 characters and `CT-22<nnn>` 8; the ids `CT-2201` to `CT-2203` of
        // scenario 22 are 7 and do not count.
        let created = kinds
            .iter()
            .filter(|kind| kind.starts_with("task.created CT-22") && kind.len() == 21)
            .count();
        t.note("a resync came", json!(!resyncs.is_empty()));
        let twice = resyncs.windows(2).any(|pair| pair[1] == pair[0] + 1);
        t.note("never two in a row", json!(!twice));
        t.note("some were skipped", json!(created < posted));
        let after = live
            && resyncs
                .last()
                .is_some_and(|at| kinds[at + 1..].contains(&"task.created CT-2131".to_string()));
        t.note("live after it", json!(after));
        t
    })
    .await;
}
