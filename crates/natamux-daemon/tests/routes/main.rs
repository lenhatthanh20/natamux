//! The terminal routes and socket of a running Rust daemon: the part-1 cases of the `terminal`,
//! `authentication` and `ended shells` groups of `server.test.ts`. The task routes are in
//! `tasks.rs`, `task_events.rs` and `task_extras.rs`; the workspace, git and GitHub routes are in
//! `workspaces.rs`, `changes.rs`, `git.rs` and `github.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod agents;
mod changes;
#[cfg(unix)]
mod fake_tools;
mod git;
mod git_repo;
mod github;
mod hook_client;
mod hooks;
mod raw_client;
mod repos;
mod server;
mod socket;
mod stages;
mod task_events;
mod task_extras;
mod tasks;
mod workspaces;

use hyper::Method;
use natamux_daemon::terminals::shell_files::{OpenShell, save_ended_shells, save_open_shells};
use natamux_proto::terminals::ended::EndedShell;
use serde_json::json;
use server::{TOKEN, daemon, daemon_in, daemon_silent_after, eventually, test_root};

#[tokio::test(flavor = "multi_thread")]
async fn serves_health_without_a_token() {
    let d = daemon("health", false).await;
    assert_eq!(
        d.call(Method::GET, "/health", None).await,
        (200, json!({ "status": "ok" }))
    );
    assert_eq!(d.call(Method::GET, "/api/health", None).await.0, 200);
}

#[tokio::test(flavor = "multi_thread")]
async fn rejects_a_request_without_a_token_or_with_a_wrong_one() {
    let d = daemon("auth", true).await;
    let unauthorized = (401, json!({ "error": "unauthorized" }));
    assert_eq!(
        d.call(Method::GET, "/tasks/T-0001/terminals", None).await,
        unauthorized
    );
    assert_eq!(
        d.call(Method::GET, "/tasks/T-0001/terminals", Some("nope"))
            .await,
        unauthorized
    );
    let in_query = format!("/tasks/T-0001/terminals?token={TOKEN}");
    assert_eq!(d.call(Method::GET, &in_query, None).await, unauthorized);
    assert_eq!(d.call(Method::GET, "/unknown", None).await, unauthorized);
}

#[tokio::test(flavor = "multi_thread")]
async fn takes_the_api_at_api_too_and_answers_404_for_an_unknown_route() {
    let d = daemon("api", true).await;
    assert_eq!(
        d.call(Method::GET, "/api/tasks/T-0001/terminals", Some(TOKEN))
            .await
            .0,
        200
    );
    assert_eq!(
        d.call(Method::GET, "/tasks/T-0001/terminals", Some("agent-token"))
            .await
            .0,
        200
    );
    let (status, body) = d.call(Method::GET, "/nothing?x=1", Some(TOKEN)).await;
    assert_eq!(status, 404);
    assert_eq!(
        body,
        json!({ "message": "Route GET:/nothing?x=1 not found", "error": "Not Found", "statusCode": 404 })
    );
}

// A raw request: a client library would reject or rewrite the "\\". Node's router does not
// read "\\" as "/" or resolve "..", and neither does this one.
#[tokio::test(flavor = "multi_thread")]
async fn decides_by_the_route_that_matched_not_by_a_path_with_a_backslash_in_it() {
    let d = daemon("backslash", true).await;
    for (method, path) in [
        ("POST", r"/hooks/claude-code\..\..\health"),
        ("GET", r"/tasks/T-0001/files/..\..\..\health"),
        ("GET", r"/tasks/T-9999/files/..\..\..\health"),
        (
            "GET",
            r"/tasks/T-0001/files/..\..\..\tasks\T-0001\terminal?ticket=x",
        ),
        (
            "GET",
            r"/tasks/T-0001/files/..\..\..\events?token=test-token",
        ),
    ] {
        assert_eq!(d.raw_status(method, path).await, 401, "{method} {path}");
    }
    // Node's test sends no token, so all answer 401 there. With the token the hook path reaches
    // its route, as agent `claude-code\..\..\health`, which no adapter has: 404, not /health.
    let status = d
        .raw_status_as("POST", r"/hooks/claude-code\..\..\health", Some(TOKEN))
        .await;
    assert_eq!(status, 404);
}

// The silence limit is 90 s; this daemon's is 600 ms, the rest is as in Node.
#[tokio::test(flavor = "multi_thread")]
async fn closes_a_socket_that_sent_nothing_for_silent_close_ms_and_its_client_detaches() {
    let root = test_root("silence");
    let d = daemon_silent_after(root, true, std::time::Duration::from_millis(600)).await;
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let mut quiet = d.open("cols=120&rows=40").await;
    quiet.until(|socket| socket.replayed()).await;
    let pty = d.ptys()[0].clone();
    assert_eq!(pty.sizes(), [(120, 40)]);

    // The first client pings as a dashboard does; the quiet one sends one message, then only
    // protocol pings, which do not count, as they do not in Node.
    let mut steps = 0;
    while quiet.open_after(100).await {
        first.send(r#"{"type":"ping"}"#).await;
        steps += 1;
        if steps == 4 {
            // Any message counts.
            quiet.send(r#"{"type":"ack","offset":0}"#).await;
        } else {
            quiet.send_ping().await;
        }
        assert!(steps < 30, "the quiet socket was never dropped");
    }
    // Dropped 600 ms after its last message, not before.
    assert!(steps >= 9, "dropped after {steps} steps");
    // The shell goes back to the size of the client that is left.
    eventually(|| pty.sizes() == [(120, 40), (80, 24)]).await;
    assert!(first.open_after(100).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn runs_a_shell_in_the_workspace_and_passes_input_and_output() {
    let d = daemon("run", true).await;
    let mut socket = d.open("cols=100&rows=30").await;
    socket.until(|socket| socket.replayed()).await;
    let pty = d.ptys()[0].clone();
    assert_eq!((pty.options.cols, pty.options.rows), (100, 30));
    assert!(pty.options.cwd.ends_with("T-0001"));

    socket.send(r#"{"type":"input","data":"pwd\r"}"#).await;
    socket.until(|socket| socket.output() == "pwd\r").await;
    socket
        .send(r#"{"type":"resize","cols":120,"rows":40}"#)
        .await;
    socket.send("not json").await;
    socket.send(r#"{"type":"input","data":42}"#).await;
    // Frames arrive in order, so once this one is echoed, the ones before it were handled.
    socket.send(r#"{"type":"input","data":"done"}"#).await;
    socket.until(|socket| socket.output() == "pwd\rdone").await;

    pty.exit(Some(0));
    assert_eq!(socket.close_code().await, 0);
    assert_eq!(
        socket.controls().last(),
        Some(&json!({ "type": "exit", "code": 0 }))
    );
    assert_eq!(pty.sizes(), [(120, 40)]);
    assert_eq!(*pty.written.lock().unwrap(), ["pwd\r", "done"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn sends_hello_first_then_the_snapshot_and_replayed_with_output_as_binary_frames() {
    let d = daemon("hello", true).await;
    let mut socket = d.open("cols=80&rows=24").await;
    socket.until(|socket| socket.replayed()).await;
    let controls = socket.controls();
    assert_eq!(socket.types()[..3], ["hello", "snapshot", "replayed"]);
    assert!(
        controls[0]["shellId"]
            .as_str()
            .is_some_and(|id| id.len() == 22)
    );
    assert_eq!(
        controls[1],
        json!({ "type": "snapshot", "cols": 80, "rows": 24 })
    );
    assert_eq!(controls[2], json!({ "type": "replayed", "offset": 0 }));

    socket.send(r#"{"type":"input","data":"ls\r"}"#).await;
    socket.until(|socket| socket.output() == "ls\r").await;
    socket.send(r#"{"type":"ping"}"#).await;
    socket
        .until(|socket| socket.types().last().is_some_and(|kind| kind == "pong"))
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn gives_the_shell_the_size_of_a_visible_client_only() {
    let d = daemon("size", true).await;
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let mut hidden = d.open("cols=100&rows=30&visible=0").await;
    hidden.until(|socket| socket.replayed()).await;
    assert!(d.ptys()[0].sizes().is_empty());

    hidden.send(r#"{"type":"visible","visible":true}"#).await;
    // The marker comes from the client that has the shell's size, so it changes nothing itself.
    first.send(r#"{"type":"input","data":"marker"}"#).await;
    first.until(|socket| socket.output() == "marker").await;
    assert!(d.ptys()[0].sizes().is_empty());

    hidden
        .send(r#"{"type":"visible","visible":true,"cols":90,"rows":20}"#)
        .await;
    let pty = d.ptys()[0].clone();
    eventually(|| pty.sizes() == [(90, 20)]).await;

    let mut shown = d.open("cols=120&rows=40&visible=1").await;
    shown.until(|socket| socket.replayed()).await;
    assert_eq!(pty.sizes(), [(90, 20), (120, 40)]);
}

#[tokio::test(flavor = "multi_thread")]
async fn ignores_a_binary_frame_from_the_client() {
    let d = daemon("binary", true).await;
    let mut socket = d.open("cols=80&rows=24").await;
    socket.until(|socket| socket.replayed()).await;
    socket.send(r#"{"type":"ack","offset":5}"#).await;
    socket
        .send_binary(br#"{"type":"input","data":"binary"}"#)
        .await;
    socket.send(r#"{"type":"input","data":"after"}"#).await;
    socket.until(|socket| socket.output() == "after").await;
    assert_eq!(*d.ptys()[0].written.lock().unwrap(), ["after"]);
    assert_eq!(socket.closed, None);
}

#[tokio::test(flavor = "multi_thread")]
async fn resumes_a_shell_by_its_id_with_only_the_output_since_the_offset() {
    let d = daemon("resume", true).await;
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let controls = first.controls();
    let (hello, done) = (controls[0].clone(), controls[2]["offset"].as_u64().unwrap());
    first.drop_connection().await;
    d.ptys()[0].emit("missed");
    let pty = d.ptys()[0].clone();
    // The ring has it once the batch went out.
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let shell_id = hello["shellId"].as_str().unwrap();
    let mut back = d
        .open(&format!(
            "cols=100&rows=30&shellId={shell_id}&offset={done}"
        ))
        .await;
    back.until(|socket| socket.replayed()).await;
    assert_eq!(
        back.controls(),
        [
            hello,
            json!({ "type": "resume", "offset": done }),
            json!({ "type": "replayed", "offset": done + 6 }),
        ]
    );
    assert_eq!(back.output(), "");
    assert_eq!(back.frames.len(), 4);
    assert_eq!(d.ptys().len(), 1);
    // A resume never takes the shell's size.
    assert!(pty.sizes().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn closes_a_resume_of_a_shell_that_no_longer_runs_with_close_gone_and_starts_none() {
    let d = daemon("gone", true).await;
    let mut socket = d.open("cols=80&rows=24&shellId=gone&offset=0").await;
    assert_eq!(socket.close_code().await, 4410);
    assert!(socket.frames.is_empty());
    assert!(d.ptys().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn answers_an_error_and_closes_when_the_shell_cannot_start_without_hello() {
    let d = daemon("refused", true).await;
    let ticket = d.ticket().await;
    d.clear_workspace().await;
    let mut socket = d
        .open_with(&format!("ticket={ticket}&cols=80&rows=24"))
        .await;
    assert_eq!(socket.close_code().await, 4409);
    assert_eq!(
        socket.controls(),
        [json!({ "type": "error", "message": "T-0001 has no workspace" })]
    );
    assert!(d.ptys().is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_no_terminals_for_a_task_that_has_none() {
    let d = daemon("none", true).await;
    let empty =
        json!({ "terminals": [], "agents": [], "lanes": {}, "programs": {}, "restoring": {} });
    assert_eq!(
        d.call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
            .await,
        (200, empty)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn opens_a_second_terminal_lists_the_terminals_and_closes_one() {
    let d = daemon("second", true).await;
    let mut first = d.open("cols=80&rows=24").await;
    first.until(|socket| socket.replayed()).await;
    let mut second = d.open("cols=80&rows=24&terminal=2").await;
    second.until(|socket| socket.replayed()).await;
    assert_eq!(d.ptys().len(), 2);

    let listed = d
        .call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
        .await;
    let both = json!({
        "terminals": ["1", "2"], "agents": [], "lanes": {},
        "programs": { "1": "bash", "2": "bash" }, "restoring": {},
    });
    assert_eq!(listed, (200, both));

    let removed = d
        .call(Method::DELETE, "/tasks/T-0001/terminals/2", Some(TOKEN))
        .await;
    assert_eq!(removed, (200, json!({ "terminals": ["1"] })));
    assert_eq!(second.close_code().await, 0);
    assert_eq!(
        second.controls().last(),
        Some(&json!({ "type": "exit", "code": null }))
    );
    let bad = d
        .call(Method::DELETE, "/tasks/T-0001/terminals/0", Some(TOKEN))
        .await;
    assert_eq!(bad.0, 400);
    assert_eq!(
        bad.1["message"],
        "terminal: A terminal is named by a number"
    );
}

// Node's server tests run on a fixed platform (linux); the Rust test daemon runs the real one,
// and on Windows a launch is typed in full, with no script.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn lists_the_shells_that_wait_to_come_back_and_brings_them_back_when_opened() {
    let root = test_root("restore");
    let open = [
        OpenShell {
            name: "6".into(),
            agent: Some("claude-code".into()),
            session: Some("a-6".into()),
            paused: false,
        },
        OpenShell {
            name: "7".into(),
            agent: None,
            session: None,
            paused: false,
        },
    ];
    save_open_shells(&root.join("data"), "T-0001", &open).unwrap();
    let d = daemon_in(root, true).await;
    let listed = d
        .call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
        .await
        .1;
    assert_eq!(listed["terminals"], json!(["6", "7"]));
    assert_eq!(
        listed["restoring"],
        json!({ "6": "claude-code", "7": null })
    );
    assert!(d.ptys().is_empty());

    // A client that lost its socket does not start a waiting shell.
    let mut lost = d
        .open("cols=80&rows=24&terminal=7&shellId=old&offset=0")
        .await;
    assert_eq!(lost.close_code().await, 4410);
    assert!(d.ptys().is_empty());

    // The plain shell opens the task: the agent of shell 6 comes back with it.
    let mut plain = d.open("cols=80&rows=24&terminal=7").await;
    plain.until(|socket| socket.replayed()).await;
    assert_eq!(d.ptys().len(), 2);
    let mut agent = d.open("cols=80&rows=24&terminal=6").await;
    agent.until(|socket| socket.replayed()).await;
    assert_eq!(d.ptys().len(), 2);
    let data = d.root.join("data");
    let script = std::fs::read_to_string(data.join("agent-launch-T-0001-6.sh")).unwrap();
    assert!(script.contains("--resume 'a-6'"), "{script}");
    assert!(!data.join("agent-launch-T-0001-7.sh").exists());
    let listed = d
        .call(Method::GET, "/tasks/T-0001/terminals", Some(TOKEN))
        .await
        .1;
    assert_eq!(listed["terminals"], json!(["6", "7"]));
    assert_eq!(listed["restoring"], json!({ "6": "claude-code" }));
}

#[tokio::test(flavor = "multi_thread")]
async fn closes_a_socket_without_a_valid_ticket() {
    let d = daemon("ticket", true).await;
    let used = d.ticket().await;
    let mut first = d.open_with(&format!("ticket={used}&cols=80&rows=24")).await;
    first.until(|socket| socket.replayed()).await;
    for query in [
        format!("ticket={used}&cols=80&rows=24"),
        "ticket=nope&cols=80&rows=24".into(),
    ] {
        let mut socket = d.open_with(&query).await;
        assert_eq!(socket.close_code().await, 4401);
    }
    // A bad query is refused before the ticket is used: the ticket still works after it.
    let fresh = d.ticket().await;
    let mut bad = d.open_with(&format!("ticket={fresh}&cols=0&rows=24")).await;
    assert_eq!(bad.close_code().await, 4401);
    let mut good = d
        .open_with(&format!("ticket={fresh}&cols=80&rows=24"))
        .await;
    good.until(|socket| socket.replayed()).await;
    assert_eq!(d.ptys().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn does_not_let_a_ticket_stand_in_for_the_token_anywhere_else() {
    let d = daemon("stand-in", true).await;
    let ticket = d.ticket().await;
    let path = format!("/tasks/T-0001/terminals?ticket={ticket}");
    assert_eq!(d.call(Method::GET, &path, None).await.0, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn needs_the_token_for_a_ticket_and_a_workspace() {
    let d = daemon("needs", false).await;
    let anonymous = d
        .call(Method::POST, "/tasks/T-0001/terminal/ticket", None)
        .await;
    assert_eq!(anonymous.0, 401);
    let none = d
        .call(Method::POST, "/tasks/T-0001/terminal/ticket", Some(TOKEN))
        .await;
    assert_eq!(
        none,
        (
            409,
            json!({ "error": "conflict", "message": "T-0001 has no workspace" })
        )
    );
    let bad = d
        .call(Method::POST, "/tasks/nope/terminal/ticket", Some(TOKEN))
        .await;
    assert_eq!(bad.0, 400);
    assert_eq!(bad.1["message"], "id: Task id looks like T-42 or ABC-42");
}

#[tokio::test(flavor = "multi_thread")]
async fn lists_the_last_screens_kept_for_a_task() {
    let d = daemon("ended", true).await;
    let path = "/tasks/T-0001/ended-shells";
    assert_eq!(
        d.call(Method::GET, path, Some(TOKEN)).await,
        (200, json!({ "shells": [] }))
    );
    let shell = EndedShell {
        name: "1".into(),
        agent: None,
        stage: None,
        session: None,
        ended_at: "2026-10-15T00:07:00.000Z".into(),
        text: "Done.".into(),
    };
    save_ended_shells(&d.root.join("data"), "T-0001", &[shell]).unwrap();
    let expected = json!({ "shells": [{
        "name": "1", "agent": null, "stage": null, "session": null,
        "endedAt": "2026-10-15T00:07:00.000Z", "text": "Done.",
    }] });
    assert_eq!(
        d.call(Method::GET, path, Some(TOKEN)).await,
        (200, expected)
    );
}
