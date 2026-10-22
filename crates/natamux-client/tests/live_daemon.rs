//! The client against a real Natamux daemon. Ignored by default; the `live-daemon` job of
//! .github/workflows/rust.yml starts the daemon image and runs these with `--ignored`.
//!
//! Env: NATAMUX_URL, NATAMUX_TOKEN, and NATAMUX_LIVE_TASK, a task with a workspace.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use futures::StreamExt;
use natamux_client::{
    Client, Config, LinkStatus, SessionOptions, TaskListEvent, TerminalEvent, TerminalHandle,
    TerminalStatus,
};

const WAIT: Duration = Duration::from_secs(20);
// Not Terminal 1, where the task's agent may run.
const TERMINAL: &str = "7";

fn client() -> (Client, String) {
    let config = Config::from_env().expect("NATAMUX_URL and NATAMUX_TOKEN");
    let task = std::env::var("NATAMUX_LIVE_TASK").expect("NATAMUX_LIVE_TASK");
    (Client::new(config).unwrap(), task)
}

/// What one socket of a terminal showed: the bytes, and the catch-up bytes apart.
#[derive(Default)]
struct Screen {
    live: Vec<u8>,
    caught_up: Vec<u8>,
    offset: u64,
}

impl Screen {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.live).into_owned()
    }
}

/// Reads events, acting as the app does, until `done` holds or WAIT passes.
async fn read_until(
    handle: &mut TerminalHandle,
    screen: &mut Screen,
    done: impl Fn(&Screen) -> bool,
) {
    let deadline = tokio::time::Instant::now() + WAIT;
    while !done(screen) {
        let event = tokio::time::timeout_at(deadline, handle.events.next())
            .await
            .expect("timed out")
            .expect("the session ended");
        match event {
            TerminalEvent::Output {
                bytes,
                end: Some(end),
            } => {
                screen.live.extend(bytes);
                screen.offset = end;
                handle.parsed(end);
            }
            TerminalEvent::Output { bytes, end: None } => screen.caught_up.extend(bytes),
            TerminalEvent::CaughtUp { offset } => {
                screen.offset = offset;
                handle.caught_up(offset);
            }
            TerminalEvent::Status(TerminalStatus::Failed(reason)) => panic!("failed: {reason}"),
            TerminalEvent::Status(TerminalStatus::Ended { code }) => panic!("ended: {code:?}"),
            TerminalEvent::Status(_) | TerminalEvent::Reset { .. } => {}
        }
    }
}

fn runtime() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap()
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN, NATAMUX_LIVE_TASK"]
fn types_into_a_shell_and_resumes_without_repeating_bytes() {
    let (client, task) = client();
    runtime().block_on(async {
        let mut handle = client.open_terminal(SessionOptions::new(&task, TERMINAL, 100, 30, true));
        let mut screen = Screen::default();
        // The shell's first prompt, then the echo of a command and its output.
        read_until(&mut handle, &mut screen, |screen| !screen.live.is_empty()).await;
        handle.input("echo natamux-$((40+2))\r");
        read_until(&mut handle, &mut screen, |screen| {
            screen.text().contains("natamux-42")
        })
        .await;

        // A lost socket resumes from its offset: nothing it already had comes again.
        let before = screen.offset;
        handle.reconnect();
        let mut after = Screen::default();
        read_until(&mut handle, &mut after, |screen| screen.offset >= before).await;
        assert!(
            !String::from_utf8_lossy(&after.caught_up).contains("natamux-42"),
            "the resume repeated old output"
        );
        handle.input("echo natamux-$((50+5))\r");
        read_until(&mut handle, &mut after, |screen| {
            screen.text().contains("natamux-55")
        })
        .await;
        assert!(!after.text().contains("natamux-42"));
    });
    runtime().block_on(async {
        client.close_terminal(&task, TERMINAL).await.unwrap();
    });
}

#[test]
#[ignore = "needs a daemon: NATAMUX_URL, NATAMUX_TOKEN, NATAMUX_LIVE_TASK"]
fn lists_tasks_live() {
    let (client, task) = client();
    runtime().block_on(async {
        let mut events = client.watch_tasks();
        loop {
            match tokio::time::timeout(WAIT, events.next()).await.unwrap() {
                Some(TaskListEvent::Tasks(tasks)) => {
                    assert!(tasks.iter().any(|listed| listed.id == task));
                    break;
                }
                Some(TaskListEvent::Status(LinkStatus::Connecting | LinkStatus::Live)) => {}
                other => panic!("unexpected {other:?}"),
            }
        }
    });
}
