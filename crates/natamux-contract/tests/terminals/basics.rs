//! Scenarios 1 to 4 and 12: routes and auth, tickets, a first shell, the input gap, ping.

use hyper::Method;
use natamux_contract::daemon::Daemon;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::Transcript;
use serde_json::json;

use crate::{both, rows, run, shell};

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn health_auth_and_unknown_routes() {
    both(
        "health and auth",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            t.note("health", json!(d.call(Method::GET, "/health", false).await));
            let path = format!("/tasks/{}/terminals", d.task);
            t.note("no token", json!(d.call(Method::GET, &path, false).await));
            t.note(
                "unknown route",
                json!(d.call(Method::GET, "/nothing", true).await),
            );
            t.note(
                "ended shells",
                json!(
                    d.call(
                        Method::GET,
                        &format!("/tasks/{}/ended-shells", d.task),
                        true
                    )
                    .await
                ),
            );
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn tickets() {
    both("tickets", &[], |d: Daemon, mut t: Transcript| async move {
        let (status, body) = d
            .call(
                Method::POST,
                &format!("/tasks/{}/terminal/ticket", d.task),
                true,
            )
            .await;
        let ticket = body["ticket"].as_str().unwrap_or_default().to_string();
        t.note("ticket", json!([status, body, ticket.len()]));
        let unknown = d
            .call(Method::POST, "/tasks/T-99999/terminal/ticket", true)
            .await;
        t.note("ticket of an unknown task", json!(unknown));
        let (status, bad) = d
            .call(Method::POST, "/tasks/nope/terminal/ticket", true)
            .await;
        let issues = bad["issues"].as_array().map_or(0, Vec::len);
        t.note(
            "bad task id",
            json!([status, bad["error"], bad["message"], issues]),
        );

        let mut first =
            Socket::open_raw(&d, &format!("ticket={ticket}&cols=80&rows=24&terminal=2")).await;
        first.until(|socket| socket.count("replayed") > 0).await;
        let mut reused =
            Socket::open_raw(&d, &format!("ticket={ticket}&cols=80&rows=24&terminal=2")).await;
        t.note(
            "reused ticket",
            json!([reused.close_code().await, reused.frames.len()]),
        );
        let mut unknown = Socket::open_raw(&d, "ticket=nope&cols=80&rows=24&terminal=2").await;
        t.note(
            "unknown ticket",
            json!([unknown.close_code().await, unknown.frames.len()]),
        );
        let fresh = d.ticket().await;
        let mut bad_size =
            Socket::open_raw(&d, &format!("ticket={fresh}&cols=0&rows=24&terminal=2")).await;
        t.note("bad size", json!(bad_size.close_code().await));
        let mut after =
            Socket::open_raw(&d, &format!("ticket={fresh}&cols=80&rows=24&terminal=2")).await;
        let works = after.until(|socket| socket.count("replayed") > 0).await;
        t.note("the ticket of a refused query still works", json!(works));
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn open_a_shell_with_its_environment() {
    both("open", &[], |d: Daemon, mut t: Transcript| async move {
        let mut socket = shell(&d, 3, "cols=80&rows=24").await;
        t.note("first messages", json!(socket.controls()[..2]));
        let print = r#"printf 'env %s|%s|%s|%s|%s|%s\n' "$NATAMUX_TASK_ID" "$NATAMUX_TERMINAL" "$PORT" "$NATAMUX_PORT" "$TERM" "$LANG"; echo "proxy $NO_PROXY""#;
        run(&mut socket, print, 1).await;
        let screen: Vec<String> = socket.screen(80, 24);
        let lines: Vec<&String> = screen.iter().filter(|line| line.starts_with("env ") || line.starts_with("proxy ")).collect();
        t.note("environment", json!(lines));
        t.note("screen", rows(&socket, 80, 24));
        t
    })
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn type_with_the_input_gap() {
    both(
        "input gap",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let mut socket = shell(&d, 4, "cols=80&rows=24").await;
            // One message with a DEL: "b" is erased before "c" comes, 25 ms apart.
            socket.input("echo ab\x7fc-$((1+1))\r").await;
            let done = socket.until(|socket| socket.text().contains("ac-2")).await;
            t.note("erased and typed", json!(done));
            run(&mut socket, "true", 1).await;
            t.note("screen", rows(&socket, 80, 24));
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn ping_and_what_is_ignored() {
    both("ping", &[], |d: Daemon, mut t: Transcript| async move {
        let mut socket = shell(&d, 13, "cols=80&rows=24").await;
        socket
            .send_binary(br#"{"type":"input","data":"binary"}"#)
            .await;
        socket.send("not json").await;
        socket.send(r#"{"type":"input","data":42}"#).await;
        socket.send(r#"{"type":"resize","cols":0,"rows":24}"#).await;
        socket.send(r#"{"type":"ping"}"#).await;
        socket.until(|socket| socket.count("pong") > 0).await;
        t.note("pong", json!(socket.controls().last()));
        run(&mut socket, "stty size", 1).await;
        t.note("screen", rows(&socket, 80, 24));
        t.note("open", json!(socket.closed.is_none()));
        t
    })
    .await;
}
