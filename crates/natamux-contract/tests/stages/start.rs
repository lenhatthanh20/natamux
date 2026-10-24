//! 23 `stage_start`, and the time a stage start takes on each daemon.

use std::time::{Duration, Instant};

use natamux_contract::daemon::{Daemon, daemons};
use natamux_contract::events::Events;
use natamux_contract::socket::Socket;
use natamux_contract::transcript::Transcript;

use crate::{
    both, cancel, launch_script, make_twins, on_task, report, stage_events, start, task, terminals,
    wait_at, wait_event, wait_file,
};

/// `start` starts the first agent stage (a new task waits there); the agent writes the spec, the
/// report comes, the checks pass, and the manual gate waits.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn stage_start() {
    make_twins(&[2301], "Stage start", "").await;
    both(
        "stage_start",
        2301,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            t.note("start", start(&d, &id).await);
            wait_event(&d, &id, "stage.started", "spec", 1).await;
            t.note("launch script", launch_script(&d, &id, "1").await);
            wait_file(&d, &id, "spec.md", "## Scope").await;
            t.note("report", report(&d, &id, "spec", "1").await);
            wait_at(&d, &id, "spec", "review").await;
            t.note("spec waits", task(&d, &id).await);
            t.note("terminals while it waits", terminals(&d, &id).await);
            t.note("events", stage_events(&d, &id).await);
            cancel(&d, &id).await;
            t
        },
    )
    .await;
}

/// Milliseconds from sending the `start` of the task's first stage to the first output of the
/// agent ("Fake agent") in Terminal 1, read by a dashboard that attaches as soon as `/events`
/// lists the shell.
async fn start_time(daemon: &Daemon, id: &str) -> u128 {
    let mut events = Events::open(daemon, Some(&daemon.token)).await;
    let started = Instant::now();
    start(daemon, id).await;
    events
        .until(|messages| {
            messages
                .iter()
                .any(|message| message["kind"] == "shells.changed" && message["taskId"] == id)
        })
        .await;
    let task = on_task(daemon, id);
    let mut socket = Socket::open(&task, "terminal=1&cols=120&rows=30&visible=1").await;
    let seen = socket
        .until(|socket| socket.text().contains("Fake agent"))
        .await;
    let time = started.elapsed().as_millis();
    assert!(seen, "{}: no agent output in {id}", daemon.name);
    drop(socket);
    time
}

fn median(mut times: Vec<u128>) -> u128 {
    times.sort_unstable();
    times[times.len() / 2]
}

/// Five starts on each daemon, taking turns. Gate: Rust's median is not above Node's. Node is
/// reached through nginx, which is in its number.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn stage_start_time() {
    let numbers = [2311, 2312, 2313, 2314, 2315];
    make_twins(&numbers, "Start time", "").await;
    let (node, rust) = daemons();
    let (mut node_times, mut rust_times) = (Vec::new(), Vec::new());
    for number in numbers {
        node_times.push(start_time(&node, &format!("CT-{number}")).await);
        rust_times.push(start_time(&rust, &format!("CR-{number}")).await);
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
    for number in numbers {
        cancel(&node, &format!("CT-{number}")).await;
        cancel(&rust, &format!("CR-{number}")).await;
    }
    let (node_median, rust_median) = (median(node_times.clone()), median(rust_times.clone()));
    let line = format!(
        "| stage_start_time | `start` to the agent's first output | Node {node_times:?} ms, median {node_median}; Rust {rust_times:?} ms, median {rust_median} |\n"
    );
    print!("{line}");
    if let Ok(path) = std::env::var("NATAMUX_CONTRACT_REPORT") {
        use std::io::Write as _;
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)
        {
            let _ = file.write_all(line.as_bytes());
        }
    }
    assert!(
        rust_median <= node_median,
        "a stage start is slower on Rust: {rust_median} ms against {node_median} ms"
    );
}
