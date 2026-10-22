use super::*;

fn open(protocol: &mut Protocol) -> Step {
    let _ = protocol.query("t".into());
    protocol.opened()
}

fn hello(protocol: &mut Protocol) {
    protocol.server(ServerMessage::Hello {
        shell_id: "s1".into(),
    });
}

fn live(protocol: &mut Protocol, bytes: usize) -> Option<u64> {
    match protocol.output(vec![b'x'; bytes]) {
        TerminalEvent::Output { end, .. } => end,
        other => panic!("not output: {other:?}"),
    }
}

#[test]
fn a_first_visible_connect_claims_the_size_in_its_query() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    let query = protocol.query("t1".into());
    assert_eq!(
        (query.cols, query.rows, query.visible, query.shell_id),
        (80, 24, true, None)
    );
    assert_eq!(query.terminal, "1");
    // The query carried the size: nothing more to send on open.
    let step = protocol.opened();
    assert!(step.send.is_empty());
    assert_eq!(step.emit, [TerminalEvent::Status(TerminalStatus::Open)]);
}

#[test]
fn a_reconnect_resumes_its_shell_from_the_known_offset() {
    let mut protocol = Protocol::new("2", 80, 24, true);
    open(&mut protocol);
    hello(&mut protocol);
    assert_eq!(
        protocol.output(b"abc".to_vec()),
        TerminalEvent::Output {
            bytes: b"abc".to_vec(),
            end: Some(3),
        }
    );
    assert_eq!(protocol.closed(None), AfterClose::Retry);
    let query = protocol.query("t2".into());
    assert_eq!(query.shell_id.as_deref(), Some("s1"));
    assert_eq!(query.offset, None, "no offset before a replayed set one");

    protocol.opened();
    protocol.server(ServerMessage::Resume { offset: 0 });
    assert_eq!(
        live(&mut protocol, 10),
        None,
        "catch-up bytes are not counted"
    );
    let step = protocol.server(ServerMessage::Replayed { offset: 13 });
    assert_eq!(step.emit, [TerminalEvent::CaughtUp { offset: 13 }]);
    assert_eq!(
        live(&mut protocol, 2),
        Some(15),
        "live bytes count from replayed"
    );
    protocol.closed(None);
    assert_eq!(protocol.query("t3".into()).offset, Some(15));
}

#[test]
fn a_resume_does_not_claim_the_size() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    hello(&mut protocol);
    protocol.closed(None);
    protocol.query("t".into());
    let step = protocol.opened();
    assert!(step.send.is_empty());
    // Typing claims it first.
    let step = protocol.input("a");
    assert_eq!(
        step.send,
        [
            ClientMessage::Resize { cols: 80, rows: 24 },
            ClientMessage::Input { data: "a".into() },
        ]
    );
}

#[test]
fn a_cut_catch_up_forgets_the_offset() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    hello(&mut protocol);
    protocol.server(ServerMessage::Replayed { offset: 100 });
    protocol.server(ServerMessage::Snapshot { cols: 90, rows: 30 });
    assert_eq!(protocol.closed(None), AfterClose::Retry);
    assert_eq!(protocol.query("t".into()).offset, None);
}

#[test]
fn a_snapshot_resets_the_screen_and_holds_size_and_typing_until_it_ends() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    let step = protocol.server(ServerMessage::Snapshot {
        cols: 100,
        rows: 30,
    });
    assert_eq!(
        step.emit,
        [TerminalEvent::Reset {
            cols: 100,
            rows: 30
        }]
    );
    assert!(protocol.resize(120, 40).send.is_empty());
    assert!(protocol.input("ls\r").send.is_empty());

    protocol.server(ServerMessage::Replayed { offset: 500 });
    let step = protocol.caught_up(500);
    assert_eq!(
        step.send,
        [
            ClientMessage::Ack { offset: 500 },
            ClientMessage::Resize {
                cols: 120,
                rows: 40
            },
            ClientMessage::Input {
                data: "ls\r".into()
            },
        ]
    );
    assert!(!protocol.in_catch_up());
}

#[test]
fn acks_every_64_kib_parsed() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    protocol.server(ServerMessage::Replayed { offset: 0 });
    protocol.caught_up(0);
    let end = live(&mut protocol, 40_000).unwrap_or(0);
    assert!(protocol.parsed(end).send.is_empty());
    let end = live(&mut protocol, 30_000).unwrap_or(0);
    assert_eq!(
        protocol.parsed(end).send,
        [ClientMessage::Ack { offset: 70_000 }]
    );
    let end = live(&mut protocol, 1).unwrap_or(0);
    assert!(protocol.parsed(end).send.is_empty());
}

#[test]
fn a_catch_up_that_never_replays_ends_by_its_timeout() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    protocol.server(ServerMessage::Resume { offset: 5 });
    protocol.input("q");
    let step = protocol.catch_up_timed_out();
    assert_eq!(step.send, [ClientMessage::Input { data: "q".into() }]);
    assert!(!protocol.in_catch_up());
}

#[test]
fn coming_on_screen_claims_the_size_and_leaving_says_so() {
    let mut protocol = Protocol::new("1", 80, 24, false);
    open(&mut protocol);
    assert!(
        protocol.resize(100, 30).send.is_empty(),
        "a hidden terminal does not resize"
    );
    assert_eq!(
        protocol.set_visible(true).send,
        [ClientMessage::Visible {
            visible: true,
            cols: Some(100),
            rows: Some(30),
        }]
    );
    assert_eq!(
        protocol.resize(110, 30).send,
        [ClientMessage::Resize {
            cols: 110,
            rows: 30
        }]
    );
    assert!(
        protocol.resize(110, 30).send.is_empty(),
        "same size twice sends once"
    );
    assert_eq!(
        protocol.set_visible(false).send,
        [ClientMessage::Visible {
            visible: false,
            cols: None,
            rows: None,
        }]
    );
}

#[test]
fn a_claim_during_a_catch_up_waits_for_its_end() {
    let mut protocol = Protocol::new("1", 80, 24, false);
    open(&mut protocol);
    protocol.server(ServerMessage::Snapshot { cols: 90, rows: 20 });
    assert!(protocol.set_visible(true).send.is_empty());
    protocol.server(ServerMessage::Replayed { offset: 1 });
    let step = protocol.caught_up(1);
    assert_eq!(
        step.send,
        [
            ClientMessage::Ack { offset: 1 },
            ClientMessage::Visible {
                visible: true,
                cols: Some(80),
                rows: Some(24),
            },
        ]
    );
}

#[test]
fn final_codes_exit_and_error_stop_for_good() {
    for code in [4401, 4409, 4410] {
        let mut protocol = Protocol::new("1", 80, 24, true);
        open(&mut protocol);
        assert!(matches!(
            protocol.closed(Some(code)),
            AfterClose::Stop(TerminalStatus::Failed(_))
        ));
        assert!(protocol.ended());
    }

    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    let step = protocol.server(ServerMessage::Exit { code: Some(2) });
    assert_eq!(
        step.emit,
        [TerminalEvent::Status(TerminalStatus::Ended {
            code: Some(2)
        })]
    );
    assert!(matches!(protocol.closed(Some(1000)), AfterClose::Stop(_)));
    assert!(
        protocol.input("x").send.is_empty(),
        "nothing goes out after the end"
    );

    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    let step = protocol.server(ServerMessage::Error {
        message: "Too many terminals.".into(),
    });
    assert_eq!(
        step.emit,
        [TerminalEvent::Status(TerminalStatus::Failed(
            "Too many terminals.".into()
        ))]
    );
}

#[test]
fn hello_starts_the_backoff_again() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    assert_eq!((protocol.next_try(), protocol.next_try()), (0, 1));
    hello(&mut protocol);
    assert_eq!(protocol.next_try(), 0);
}

#[test]
fn nothing_goes_out_while_the_socket_is_down() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    protocol.closed(None);
    assert!(protocol.input("x").send.is_empty());
    assert!(protocol.resize(100, 50).send.is_empty());
    // The next query carries the new size.
    let query = protocol.query("t".into());
    assert_eq!((query.cols, query.rows), (100, 50));
}

#[test]
fn a_big_paste_goes_out_in_chunks() {
    let mut protocol = Protocol::new("1", 80, 24, true);
    open(&mut protocol);
    let paste = "y".repeat(70_000);
    let step = protocol.input(&paste);
    assert_eq!(step.send.len(), 2);
}
