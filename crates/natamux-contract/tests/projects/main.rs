//! The project scenarios of part 6 (54 to 57 of the Rust migration plan), each run on the Node
//! daemon and on the Rust daemon and compared. Each daemon has a project folder of its own in the
//! shared volume (`/data/project-node`, `/data/project-rust`: a git repository with a skill in
//! `.claude` and one in `.agents`) and a plain folder (`/data/plain-node`, `/data/plain-rust`),
//! which the daemon-contract CI job makes. Both daemons add their folder under the same name, so
//! the project id is the same; the folders become placeholders.
//!
//! A scenario removes its project at the end, so the next one (and a rerun) starts without it.
//! Twin task ids `CT-<n>` / `CR-<n>` with `n` from 5401 to 5699 are used here.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod repos;
mod settings;

use std::future::Future;
use std::time::Duration;

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token, daemons};
use natamux_contract::events::Events;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::{Allowed, Transcript, compare};
use serde_json::{Value, json};

/// The name every scenario adds its project with, so its id is the same on both daemons.
pub const PROJECT: &str = "contract-project";

/// The git folder of this daemon.
pub fn project_dir(daemon: &Daemon) -> String {
    format!("/data/project-{}", daemon.name)
}

/// The folder of this daemon that is not a git repository.
pub fn plain_dir(daemon: &Daemon) -> String {
    format!("/data/plain-{}", daemon.name)
}

/// The id of the twin task of `number` on this daemon.
pub fn twin(daemon: &Daemon, number: u32) -> String {
    match daemon.name {
        "node" => format!("CT-{number}"),
        _ => format!("CR-{number}"),
    }
}

/// Runs `scenario` on both daemons, the Node one first, and compares what each saw. The two
/// folders of the daemon become `<project>` and `<plain>` in what it notes.
pub async fn both<F, Fut>(name: &str, allowed: &[Allowed], scenario: F)
where
    F: Fn(Daemon, Transcript) -> Fut,
    Fut: Future<Output = Transcript>,
{
    let (node, rust) = daemons();
    let mut results = Vec::new();
    for daemon in [node, rust] {
        let mut transcript = Transcript::new(daemon.name);
        transcript.alias(&project_dir(&daemon), "<project>");
        transcript.alias(&plain_dir(&daemon), "<plain>");
        results.push(scenario(daemon, transcript).await);
    }
    compare(name, &results[0], &results[1], allowed);
}

/// What a scenario keeps of an answer: the status and the body.
pub fn kept(answer: &Answer) -> Value {
    json!([answer.status, answer.body])
}

pub async fn call(daemon: &Daemon, method: Method, path: &str, body: Body) -> Answer {
    daemon.request(method, path, body, Token::Daemon).await
}

pub async fn post(daemon: &Daemon, path: &str, body: Value) -> Answer {
    call(daemon, Method::POST, path, Body::Json(body)).await
}

pub async fn get(daemon: &Daemon, path: &str) -> Answer {
    call(daemon, Method::GET, path, Body::None).await
}

/// Adds the git folder of the daemon as `PROJECT`.
pub async fn add_project(daemon: &Daemon) -> Answer {
    let body = json!({ "path": project_dir(daemon), "name": PROJECT });
    post(daemon, "/repos", body).await
}

/// Removes `PROJECT`, and says nothing of how it went: a scenario starts and ends with it.
pub async fn drop_project(daemon: &Daemon) {
    call(
        daemon,
        Method::DELETE,
        &format!("/repos/{PROJECT}"),
        Body::None,
    )
    .await;
}

/// How many `repos.changed` the socket got since the last call. A task made and then deleted
/// marks the end of what came before: messages arrive in the order they were sent, so no wait
/// for "nothing" is needed. `marker` is the number of its twin id.
pub async fn repos_changed(daemon: &Daemon, events: &mut Events, marker: u32) -> usize {
    let id = twin(daemon, marker);
    let made = post(
        daemon,
        "/tasks",
        json!({ "title": format!("{id}: Marker") }),
    )
    .await;
    assert_eq!(made.status, 201, "{}: {id}: {}", daemon.name, made.body);
    let came = events
        .until(|messages| {
            messages.iter().any(|message| {
                message["kind"] == "task.created" && message["task"]["id"] == id.as_str()
            })
        })
        .await;
    assert!(came, "{}: no task.created for {id}", daemon.name);
    call(daemon, Method::DELETE, &format!("/tasks/{id}"), Body::None).await;
    let count = events
        .messages
        .iter()
        .filter(|message| message["kind"] == "repos.changed")
        .count();
    events.messages.clear();
    count
}

/// The token the daemon gives its shells: what an agent sends with. Read from a shell of the
/// part-1 task, which is then closed (the helper of the tasks and stages suites).
pub async fn agent_token(daemon: &Daemon) -> String {
    let mut socket = Socket::open(daemon, "terminal=9&cols=200&rows=20").await;
    socket.until(|socket| socket.count("replayed") > 0).await;
    tokio::time::sleep(Duration::from_millis(500)).await;
    socket
        .input("printf 'TOK<%s>\\n' \"$NATAMUX_TOKEN\"\r")
        .await;
    let found = socket
        .until(|socket| socket.text().contains("TOK<") && socket.text().matches('>').count() > 1)
        .await;
    assert!(found, "{}: no token in {:?}", daemon.name, socket.text());
    let text = socket.text();
    let token = text
        .rsplit("TOK<")
        .next()
        .and_then(|rest| rest.split('>').next())
        .unwrap_or_default()
        .to_string();
    call(
        daemon,
        Method::DELETE,
        &format!("/tasks/{}/terminals/9", daemon.task),
        Body::None,
    )
    .await;
    token
}
