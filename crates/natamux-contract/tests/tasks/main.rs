//! The task scenarios of part 2 (13 to 22 of docs/rust-phase4.md), each run on the Node daemon
//! and on the Rust daemon and compared. Task ids come from the titles (`CT-1301: …`), so both
//! daemons give the same; times and the numbers of generated ids become placeholders. They need
//! both daemons and the contract pipeline in the repository's `.natamux.json` (the daemon-contract
//! CI job), so they are ignored by default; run them one at a time.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod create;
mod events;
mod files;
mod shared;
mod socket;

use std::future::Future;
use std::time::Duration;

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token, daemons};
use natamux_contract::socket::Socket;
use natamux_contract::transcript::{Allowed, Transcript, compare};
use serde_json::{Map, Value, json};

/// Runs `scenario` on both daemons, the Node one first, and compares what each saw.
pub async fn both<F, Fut>(name: &str, allowed: &[Allowed], scenario: F)
where
    F: Fn(Daemon, Transcript) -> Fut,
    Fut: Future<Output = Transcript>,
{
    let (node, rust) = daemons();
    let left = scenario(node, Transcript::new("node")).await;
    let right = scenario(rust, Transcript::new("rust")).await;
    compare(name, &left, &right, allowed);
}

/// `2026-10-23T09:00:00.000Z` and the like.
fn is_time(text: &str) -> bool {
    let b = text.as_bytes();
    b.len() >= 20
        && b[4] == b'-'
        && b[10] == b'T'
        && b[b.len() - 1] == b'Z'
        && b[..4].iter().all(u8::is_ascii_digit)
}

/// Times become `<time>`; `seq` numbers count from the first one of their list.
pub fn clean(value: Value) -> Value {
    match value {
        Value::String(text) if is_time(&text) => json!("<time>"),
        Value::Array(items) => {
            let first = items.iter().find_map(|item| item["seq"].as_u64());
            Value::Array(
                items
                    .into_iter()
                    .map(|item| {
                        let mut item = clean(item);
                        if let (Some(first), Some(seq)) = (first, item["seq"].as_u64()) {
                            item["seq"] = json!(seq - first);
                        }
                        item
                    })
                    .collect(),
            )
        }
        Value::Object(map) => Value::Object(
            map.into_iter()
                .map(|(key, each)| (key, clean(each)))
                .collect::<Map<_, _>>(),
        ),
        other => other,
    }
}

/// The times in a text (a task file, an event log) as `<time>`.
pub fn clean_text(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    while !rest.is_empty() {
        let candidate = rest
            .get(..24)
            .filter(|part| is_time(part) && part.as_bytes()[19] == b'.');
        match candidate {
            Some(_) => {
                out.push_str("<time>");
                rest = &rest[24..];
            }
            None => {
                let next = rest.chars().next().map_or(1, char::len_utf8);
                out.push_str(&rest[..next]);
                rest = &rest[next..];
            }
        }
    }
    out
}

/// What a scenario keeps of an answer: the status and the cleaned body.
pub fn kept(answer: &Answer) -> Value {
    json!([answer.status, clean(answer.body.clone())])
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

/// The token the daemon gives its shells: what an agent sends with. Read from a shell of the
/// part-1 task, which is then closed.
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

/// The text of a file in the data folder of the daemon.
pub async fn data_file(daemon: &Daemon, path: &str) -> String {
    daemon
        .exec(&format!("cat '{}/{path}' 2>/dev/null", daemon.data_dir()))
        .await
}
