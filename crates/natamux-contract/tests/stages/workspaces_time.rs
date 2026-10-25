//! 52 `workspace_ready_time` and 53 `diff_stats_time`, in the `workspaces` module (the CI
//! filter `workspaces::`). They measure; they never sleep to wait for a state.

use std::time::Instant;

use natamux_contract::daemon::{Daemon, daemons};
use natamux_contract::events::Events;
use serde_json::json;

use super::{make_task, median, open_workspace, repo, report_row, retire, save};
use crate::{get, post, twin};

/// Milliseconds from `POST /workspaces` to the `workspace.preparing` message with no step, read
/// on `/events`.
async fn ready_time(d: &Daemon, number: u32) -> u128 {
    let id = twin(d, number);
    let mut events = Events::open(d, Some(&d.token)).await;
    let body = json!({ "title": format!("{id}: Ready time"), "repo": repo() });
    let started = Instant::now();
    let made = post(d, "/workspaces", body).await;
    let ready = |m: &serde_json::Value| {
        m["kind"] == "workspace.preparing" && m["taskId"] == id && m["step"].is_null()
    };
    let seen = events.next(ready).await;
    let time = started.elapsed().as_millis();
    assert_eq!(made.status, 202, "{}: {}", d.name, made.body);
    assert!(seen, "{}: {id} never got ready", d.name);
    time
}

/// Five new workspaces on each daemon, taking turns. Gate: Rust's median is not above Node's.
/// Node is reached through nginx, which is in its number.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn workspace_ready_time() {
    let numbers = [5201, 5202, 5203, 5204, 5205];
    let (node, rust) = daemons();
    let (mut node_times, mut rust_times) = (Vec::new(), Vec::new());
    for number in numbers {
        node_times.push(ready_time(&node, number).await);
        rust_times.push(ready_time(&rust, number).await);
    }
    for number in numbers {
        retire(&node, &twin(&node, number)).await;
        retire(&rust, &twin(&rust, number)).await;
    }
    let (node_median, rust_median) = (median(node_times.clone()), median(rust_times.clone()));
    report_row(&format!(
        "| workspace_ready_time | `POST /workspaces` to its `workspace.preparing` with no step | Node {node_times:?} ms, median {node_median}; Rust {rust_times:?} ms, median {rust_median} |\n"
    ));
    assert!(
        rust_median <= node_median,
        "a new workspace is slower on Rust: {rust_median} ms against {node_median} ms"
    );
}

/// Milliseconds for `GET /diffstats` after a file of one workspace was saved.
async fn stats_time(d: &Daemon, id: &str, round: usize) -> u128 {
    let saved = save(d, id, "README.md", &format!("round {round}\n")).await;
    assert_eq!(saved.status, 200, "{}: {}", d.name, saved.body);
    let started = Instant::now();
    let stats = get(d, "/diffstats").await;
    let time = started.elapsed().as_millis();
    assert_eq!(stats.status, 200, "{}: {}", d.name, stats.body);
    assert!(
        !stats.body["stats"][id].is_null(),
        "{}: no stat for {id}: {}",
        d.name,
        stats.body
    );
    time
}

/// A save, then `GET /diffstats`, five times on each daemon, taking turns, with five workspaces
/// per daemon (only the saved one is read again). Gate: Rust's median is not above Node's.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn diff_stats_time() {
    let numbers = [5301, 5302, 5303, 5304, 5305];
    let (node, rust) = daemons();
    for d in [&node, &rust] {
        for number in numbers {
            let id = make_task(d, number, "Diff stats").await;
            open_workspace(d, &id).await;
        }
    }
    let (mut node_times, mut rust_times) = (Vec::new(), Vec::new());
    for (round, number) in numbers.into_iter().enumerate() {
        node_times.push(stats_time(&node, &twin(&node, number), round).await);
        rust_times.push(stats_time(&rust, &twin(&rust, number), round).await);
    }
    for number in numbers {
        retire(&node, &twin(&node, number)).await;
        retire(&rust, &twin(&rust, number)).await;
    }
    let (node_median, rust_median) = (median(node_times.clone()), median(rust_times.clone()));
    report_row(&format!(
        "| diff_stats_time | `GET /diffstats` after a save | Node {node_times:?} ms, median {node_median}; Rust {rust_times:?} ms, median {rust_median} |\n"
    ));
    assert!(
        rust_median <= node_median,
        "diff stats are slower on Rust: {rust_median} ms against {node_median} ms"
    );
}
