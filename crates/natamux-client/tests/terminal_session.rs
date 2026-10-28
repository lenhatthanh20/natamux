//! Terminal sessions against the fake daemon: ticket, catch-up, ack, ping, final codes.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use std::time::Duration;

use futures::StreamExt;
use natamux_client::{
    Client, Config, SessionOptions, TerminalEvent, TerminalHandle, TerminalStatus,
};
use natamux_proto::terminals::messages::ClientMessage;
use support::{FakeDaemon, TOKEN, runtime, within};

async fn next_event(handle: &mut TerminalHandle) -> TerminalEvent {
    within(handle.events.next()).await.unwrap()
}

/// The next event that is not Connecting, Open or Reconnecting.
async fn next_non_link_event(handle: &mut TerminalHandle) -> TerminalEvent {
    loop {
        match next_event(handle).await {
            TerminalEvent::Status(
                TerminalStatus::Connecting | TerminalStatus::Open | TerminalStatus::Reconnecting,
            ) => {}
            event => return event,
        }
    }
}

fn options() -> SessionOptions {
    SessionOptions::new("T-1", "1", 80, 24, true)
}

#[test]
fn connects_with_a_ticket_and_resumes_after_a_loss() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut handle = client.open_terminal(options());
        let mut conn = daemon.next_terminal().await;
        assert_eq!(
            conn.target,
            "/tasks/T-1/terminal?ticket=t1&cols=80&rows=24&terminal=1&visible=1"
        );
        conn.text(r#"{"type":"hello","shellId":"s1"}"#).await;
        conn.text(r#"{"type":"replayed","offset":0}"#).await;
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::CaughtUp { offset: 0 }
        );
        handle.caught_up(0);
        assert_eq!(conn.recv(false).await, ClientMessage::Ack { offset: 0 });
        conn.binary(b"hello").await;
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::Output {
                bytes: b"hello".to_vec(),
                end: Some(5)
            }
        );
        // The daemon goes away without a close frame.
        drop(conn);
        let conn = daemon.next_terminal().await;
        assert_eq!(
            conn.target,
            "/tasks/T-1/terminal?ticket=t2&cols=80&rows=24&terminal=1&visible=1&shellId=s1&offset=5"
        );
    });
    assert_eq!(daemon.state(|state| state.tickets), 2);
}

#[test]
fn a_snapshot_resets_the_screen_and_acks_after_the_catch_up_and_every_64_kib() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut handle = client.open_terminal(options());
        let mut conn = daemon.next_terminal().await;
        conn.text(r#"{"type":"hello","shellId":"s1"}"#).await;
        conn.text(r#"{"type":"snapshot","cols":100,"rows":30}"#)
            .await;
        conn.binary(b"old screen").await;
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::Reset {
                cols: 100,
                rows: 30
            }
        );
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::Output {
                bytes: b"old screen".to_vec(),
                end: None
            }
        );
        // Typing during the catch-up waits for its end.
        handle.input("ls\r");
        conn.text(r#"{"type":"replayed","offset":1000}"#).await;
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::CaughtUp { offset: 1000 }
        );
        handle.caught_up(1000);
        assert_eq!(conn.recv(false).await, ClientMessage::Ack { offset: 1000 });
        assert_eq!(
            conn.recv(false).await,
            ClientMessage::Input {
                data: "ls\r".into()
            }
        );

        conn.binary(&vec![b'x'; 70_000]).await;
        let end = loop {
            if let TerminalEvent::Output { end: Some(end), .. } =
                next_non_link_event(&mut handle).await
            {
                break end;
            }
        };
        assert_eq!(end, 71_000);
        handle.parsed(end);
        assert_eq!(
            conn.recv(false).await,
            ClientMessage::Ack { offset: 71_000 }
        );
    });
}

#[test]
fn pings_and_drops_a_socket_that_never_answers() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut options = options();
        options.heartbeat = Duration::from_millis(200);
        let handle = client.open_terminal(options);
        let mut conn = daemon.next_terminal().await;
        conn.text(r#"{"type":"hello","shellId":"s1"}"#).await;
        assert_eq!(conn.recv(true).await, ClientMessage::Ping);
        // No pong: after two heartbeats the client gives up on this socket and comes back.
        let again = daemon.next_terminal().await;
        assert!(again.target.contains("shellId=s1"));
        assert_eq!(
            handle.last_loss().as_deref(),
            Some("no pong for two heartbeats")
        );
        drop(conn);
    });
}

#[test]
fn frames_that_wait_in_the_socket_reach_the_app_in_order_before_a_control_message() {
    let rt = runtime();
    let mut daemon = rt.block_on(FakeDaemon::start());
    let client = daemon.client();
    rt.block_on(async {
        let mut handle = client.open_terminal(options());
        let mut conn = daemon.next_terminal().await;
        conn.text(r#"{"type":"hello","shellId":"s1"}"#).await;
        conn.text(r#"{"type":"replayed","offset":0}"#).await;
        assert_eq!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::CaughtUp { offset: 0 }
        );
        conn.binary(b"one ").await;
        conn.binary(b"two ").await;
        conn.binary(b"three").await;
        conn.text(r#"{"type":"exit","code":0}"#).await;
        // Frames already in the socket join into fewer events; bytes, offsets and the place of
        // the exit stay the same.
        let mut bytes = Vec::new();
        let mut events = 0;
        let last_end = loop {
            match next_non_link_event(&mut handle).await {
                TerminalEvent::Output { bytes: more, end } => {
                    bytes.extend(more);
                    events += 1;
                    assert_eq!(end, Some(bytes.len() as u64));
                }
                TerminalEvent::Status(TerminalStatus::Ended { code }) => {
                    assert_eq!(code, Some(0));
                    break bytes.len();
                }
                other => panic!("unexpected {other:?}"),
            }
        };
        assert_eq!(bytes, b"one two three");
        assert_eq!(last_end, 13);
        assert!((1..=3).contains(&events));
    });
}

#[test]
fn final_close_codes_exit_and_error_do_not_reconnect() {
    for (script, expected) in [
        (
            Script::Close(4410),
            TerminalStatus::Failed("The shell of this terminal no longer runs.".into()),
        ),
        (
            Script::Close(4401),
            TerminalStatus::Failed(
                "The daemon refused the terminal: its ticket was not valid.".into(),
            ),
        ),
        (
            Script::Text(r#"{"type":"exit","code":3}"#),
            TerminalStatus::Ended { code: Some(3) },
        ),
        (
            Script::Text(r#"{"type":"error","message":"Too many terminals."}"#),
            TerminalStatus::Failed("Too many terminals.".into()),
        ),
    ] {
        let rt = runtime();
        let mut daemon = rt.block_on(FakeDaemon::start());
        let client = daemon.client();
        rt.block_on(async {
            let mut handle = client.open_terminal(options());
            let mut conn = daemon.next_terminal().await;
            conn.text(r#"{"type":"hello","shellId":"s1"}"#).await;
            match script {
                Script::Close(code) => conn.close(code).await,
                Script::Text(text) => conn.text(text).await,
            }
            assert_eq!(
                next_non_link_event(&mut handle).await,
                TerminalEvent::Status(expected)
            );
            tokio::time::sleep(Duration::from_millis(800)).await;
        });
        assert_eq!(daemon.state(|state| state.tickets), 1, "no second connect");
    }
}

enum Script {
    Close(u16),
    Text(&'static str),
}

#[test]
fn a_daemon_that_is_not_there_fails_the_first_connect() {
    let rt = runtime();
    let client = Client::new(Config {
        base_url: "http://127.0.0.1:9".into(),
        token: TOKEN.into(),
    })
    .unwrap();
    rt.block_on(async {
        let mut handle = client.open_terminal(options());
        assert!(matches!(
            next_non_link_event(&mut handle).await,
            TerminalEvent::Status(TerminalStatus::Failed(_))
        ));
    });
}
