//! T-0373 Part 6: `GET /security` and `PUT /security`, and the tamper check of a running daemon.
//! Each daemon reads its own data folder, so these tests do not change the mode of others.

use std::sync::{Arc, Mutex};

use hyper::Method;
use natamux_core::security::OUTSIDE_CHANGE_WARNING;
use serde_json::{Value, json};

use crate::server::{TOKEN, TestDaemon, daemon, eventually, start_daemon, test_root};

fn file_text(d: &TestDaemon) -> Option<String> {
    std::fs::read_to_string(d.root.join("data/security.json")).ok()
}

/// The `security.changed` messages of the daemon's bus.
fn told(d: &TestDaemon) -> Arc<Mutex<Vec<String>>> {
    let got: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&got);
    d.daemon.state.bus.subscribe(move |message| {
        if message.kind() == "security.changed" {
            sink.lock().unwrap().push(message.to_json());
        }
    });
    got
}

#[tokio::test(flavor = "multi_thread")]
async fn personal_without_a_file_and_strict_after_a_save_from_the_desktop() {
    let d = daemon("security-save", true).await;
    let messages = told(&d);
    let (status, body) = d.call(Method::GET, "/security", Some(TOKEN)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["strict"], false);
    assert_eq!(body["keys"]["ciPush"]["blocked"], false);
    assert!(body["warning"].is_null());

    let (status, body) = d
        .send(
            Method::PUT,
            "/security",
            Some(TOKEN),
            &json!({ "strict": true, "ciPush": "forbid" }),
        )
        .await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["strict"], true);
    assert_eq!(body["keys"]["ciPush"]["value"], "forbid");
    assert_eq!(body["keys"]["ciPush"]["blocked"], true);
    assert!(body["keys"]["agentAutoMode"]["reason"].is_string());
    assert!(file_text(&d).unwrap().contains("\"forbid\""));
    assert_eq!(d.call(Method::GET, "/security", Some(TOKEN)).await.1, body);
    let told = messages.lock().unwrap().clone();
    assert_eq!(
        told,
        [r#"{"kind":"security.changed","strict":true,"warning":null}"#]
    );

    // Turning it off works through the same route.
    let (status, body) = d
        .send(
            Method::PUT,
            "/security",
            Some(TOKEN),
            &json!({ "strict": false }),
        )
        .await;
    assert_eq!((status, body["strict"].clone()), (200, json!(false)));
}

#[tokio::test(flavor = "multi_thread")]
async fn a_task_token_reads_but_never_writes_and_a_ticket_cannot_write() {
    let d = daemon("security-agent", true).await;
    d.send(
        Method::PUT,
        "/security",
        Some(TOKEN),
        &json!({ "strict": true }),
    )
    .await;
    let before = file_text(&d);
    let agent = d.agent_token("T-0001");
    let (status, body) = d.call(Method::GET, "/security", Some(&agent)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["strict"], true);
    let off = json!({ "strict": false });
    let (status, _) = d.send(Method::PUT, "/security", Some(&agent), &off).await;
    assert_eq!(status, 403);
    let ticket = d.ticket().await;
    let path = format!("/security?ticket={ticket}");
    let (status, _) = d.send(Method::PUT, &path, None, &off).await;
    assert!(status == 401 || status == 403, "{status}");
    assert_eq!(file_text(&d), before, "the file is unchanged");
    assert_eq!(
        d.call(Method::GET, "/security", Some(TOKEN)).await.1["strict"],
        true
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_looser_key_or_a_bad_body_is_refused_and_writes_nothing() {
    let d = daemon("security-bad", false).await;
    for body in [
        json!({ "strict": true, "ciPush": "snapshot" }),
        json!({ "strict": true, "agents": { "permissionMode": "auto" } }),
        json!({ "ciPush": "forbid" }),
        json!([]),
    ] {
        let (status, answer) = d.send(Method::PUT, "/security", Some(TOKEN), &body).await;
        assert_eq!(status, 400, "{body}: {answer}");
    }
    assert_eq!(file_text(&d), None);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_outside_before_the_start_keeps_strict_and_warns() {
    let root = test_root("security-start");
    std::fs::write(root.join("data/security.json"), r#"{"strict": true}"#).unwrap();
    let d = start_daemon(
        root,
        None,
        natamux_proto::terminals::messages::SILENT_CLOSE,
        None,
    )
    .await;
    let (status, body) = d.call(Method::GET, "/security", Some(TOKEN)).await;
    assert_eq!(status, 200, "{body}");
    assert_eq!(body["strict"], true);
    assert_eq!(body["warning"], OUTSIDE_CHANGE_WARNING);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_file_changed_outside_while_running_keeps_strict_tells_and_warns() {
    let d = daemon("security-running", false).await;
    d.send(
        Method::PUT,
        "/security",
        Some(TOKEN),
        &json!({ "strict": true }),
    )
    .await;
    let messages = told(&d);
    std::fs::write(d.root.join("data/security.json"), r#"{"strict": false}"#).unwrap();
    eventually(|| {
        messages
            .lock()
            .unwrap()
            .iter()
            .any(|text| text.contains(OUTSIDE_CHANGE_WARNING))
    })
    .await;
    let (_, body): (u16, Value) = d.call(Method::GET, "/security", Some(TOKEN)).await;
    assert_eq!(body["strict"], true);
    assert_eq!(body["warning"], OUTSIDE_CHANGE_WARNING);
    // A save from the desktop clears the warning.
    let (_, body) = d
        .send(
            Method::PUT,
            "/security",
            Some(TOKEN),
            &json!({ "strict": true }),
        )
        .await;
    assert!(body["warning"].is_null(), "{body}");
}
