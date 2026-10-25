//! The terminal scenarios of part 1, each run on the Node daemon and on the Rust daemon and
//! compared. They need both daemons (the daemon-contract CI job), so they are ignored by
//! default; run them one at a time (`--test-threads=1`): some restart a daemon.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod basics;
mod flow;
mod shells;

use std::future::Future;

use natamux_contract::daemon::{Daemon, daemons};
use natamux_contract::socket::Socket;
use natamux_contract::transcript::{Allowed, Transcript, compare};
use serde_json::json;

/// Runs `scenario` on both daemons, the Node one first, and compares what each saw.
pub async fn both<F, Fut>(name: &str, allowed: &[Allowed], scenario: F)
where
    F: Fn(Daemon, Transcript) -> Fut,
    Fut: Future<Output = Transcript>,
{
    let (node, rust) = daemons();
    node.close_all().await;
    rust.close_all().await;
    let left = scenario(node.clone(), Transcript::new("node")).await;
    let right = scenario(rust.clone(), Transcript::new("rust")).await;
    node.close_all().await;
    rust.close_all().await;
    compare(name, &left, &right, allowed);
}

/// A socket to `terminal` that got its first screen, with the shell's prompt on it. The Rust
/// daemon sends the screen of a new shell before bash printed anything, Node a moment later:
/// typing before the prompt would make the screens differ by the order of echo and prompt.
pub async fn shell(daemon: &Daemon, terminal: u32, query: &str) -> Socket {
    let mut socket = Socket::open(daemon, &format!("terminal={terminal}&{query}")).await;
    let ready = socket
        .until(|socket| {
            socket.count("replayed") > 0
                && socket
                    .screen(200, 50)
                    .last()
                    .is_some_and(|line| line.ends_with('$'))
        })
        .await;
    assert!(
        ready,
        "{}: no replayed on terminal {terminal}: {:?}",
        daemon.name, socket.frames
    );
    socket
}

/// Types `command`, then waits for its end: a marker the shell prints, and the next prompt.
/// The marker is computed by the shell, so its echo does not count.
pub async fn run(socket: &mut Socket, command: &str, n: u32) {
    socket
        .input(&format!("{command}; echo END-$(({n}+0))\r"))
        .await;
    let marker = format!("END-{n}");
    let done = socket
        .until(|socket| {
            let text = socket.text();
            text.rsplit_once(&marker)
                .is_some_and(|(_, after)| after.trim_end().ends_with('$'))
        })
        .await;
    assert!(done, "no {marker} after {command}: {:?}", socket.text());
}

/// The screen rows that are not empty, for a compact transcript.
pub fn rows(socket: &Socket, cols: u16, rows: u16) -> serde_json::Value {
    let lines: Vec<String> = socket
        .screen(cols, rows)
        .into_iter()
        .filter(|line| !line.is_empty())
        .collect();
    json!(lines)
}
