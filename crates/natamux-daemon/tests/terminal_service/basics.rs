//! `describe('TerminalService')`.

use natamux_daemon::terminals::limits::MAX_SESSIONS;
use natamux_daemon::terminals::service::AttachOptions;
use natamux_daemon::terminals::types::TerminalError;
use natamux_proto::terminals::history::scrollback_lines;
use natamux_proto::terminals::messages::ServerMessage;

use crate::support::{SIZE, SetupOptions, Size, client, is_hello, numbered, setup, setup_with};

#[test]
fn starts_one_shell_in_the_workspace_with_the_task_id_and_the_size() {
    let h = setup();
    let (_, first) = client();
    let (_, second) = client();
    h.attach(&first);
    h.attach(&second);

    assert_eq!(h.ptys().len(), 1);
    let pty = h.pty(0);
    {
        let state = pty.state();
        assert_eq!(state.options.cwd.to_str(), Some("/work/T-0001"));
        assert_eq!((state.options.cols, state.options.rows), (80, 24));
    }
    for (name, value) in [
        ("PATH", "/bin"),
        ("NATAMUX_TOKEN", "secret"),
        ("NATAMUX_TASK_ID", "T-0001"),
        ("NATAMUX_PORT", "20010"),
        ("PORT", "20010"),
        ("TERM", "xterm-256color"),
    ] {
        assert_eq!(pty.env(name).as_deref(), Some(value), "{name}");
    }
    assert!(h.service.is_open("T-0001", "1"));
}

#[test]
fn lets_the_shell_reach_the_daemon_on_loopback_past_a_proxy() {
    let h = setup();
    let (_, one) = client();
    h.attach(&one);

    let pty = h.pty(0);
    assert_eq!(
        pty.env("NO_PROXY").as_deref(),
        Some("127.0.0.1,localhost,::1")
    );
    assert_eq!(
        pty.env("no_proxy").as_deref(),
        Some("127.0.0.1,localhost,::1")
    );
    assert_eq!(pty.env("NATAMUX_TERMINAL").as_deref(), Some("1"));
}

#[test]
fn keeps_the_hosts_the_user_already_has_in_no_proxy() {
    let env = [("PATH", "/bin"), ("NO_PROXY", ".corp.example.com")]
        .iter()
        .map(|(name, value)| (name.to_string(), value.to_string()))
        .collect();
    let h = setup_with(SetupOptions {
        env: Some(env),
        ..SetupOptions::default()
    });
    let (_, one) = client();
    h.attach(&one);

    let list = ".corp.example.com,127.0.0.1,localhost,::1";
    assert_eq!(h.pty(0).env("NO_PROXY").as_deref(), Some(list));
    assert_eq!(h.pty(0).env("no_proxy").as_deref(), Some(list));
}

#[test]
fn passes_input_and_size_to_the_shell_and_output_to_every_client() {
    let h = setup();
    let (first, first_ref) = client();
    let (second, second_ref) = client();
    h.attach(&first_ref);
    h.attach(&second_ref);

    h.service.input("T-0001", "1", None, "ls\r");
    h.service.resize(
        "T-0001",
        "1",
        None,
        Size {
            cols: 120,
            rows: 40,
        },
    );
    h.settled("T-0001", "1");

    assert_eq!(h.pty(0).written(), ["ls\r"]);
    assert_eq!(h.pty(0).sizes(), [(120, 40)]);
    for each in [&first, &second] {
        assert_eq!(each.output(), "ls\r");
        let controls = each.controls();
        assert!(is_hello(&controls[0]));
        assert_eq!(
            controls[1..],
            [
                ServerMessage::Snapshot { cols: 80, rows: 24 },
                ServerMessage::Replayed { offset: 0 },
            ]
        );
    }
}

#[test]
fn gives_a_client_that_attaches_later_the_screen_so_far_first() {
    let h = setup();
    let (_, first) = client();
    let detach = h.attach(&first);
    h.pty(0).emit("hello ");
    h.pty(0).emit("world");
    detach.detach();

    let (later, later_ref) = client();
    h.attach(&later_ref);
    h.settled("T-0001", "1");

    assert_eq!(later.screen(SIZE), ["hello world"]);
    assert_eq!(h.ptys().len(), 1);
}

#[test]
fn keeps_the_newest_lines_as_many_as_the_dashboard_keeps_above_its_screen() {
    let h = setup();
    let (_, first) = client();
    h.attach(&first);
    let kept = scrollback_lines(SIZE.cols);
    let count = kept + 500;
    h.pty(0).emit(numbered(count));

    let (later, later_ref) = client();
    h.attach(&later_ref);
    h.settled("T-0001", "1");

    let rows = later.screen(SIZE);
    assert!(rows.len() <= kept + usize::from(SIZE.rows));
    assert!(rows.contains(&format!("line {}", count - kept)));
    assert!(!rows.contains(&"line 1".to_string()));
    assert_eq!(rows.last(), Some(&format!("line {count}")));
}

#[test]
fn tells_the_clients_when_the_shell_ends_and_starts_a_new_one_next_time() {
    let h = setup();
    let (first, first_ref) = client();
    h.attach(&first_ref);

    h.pty(0).exit(Some(0));

    // Rust: the snapshot of an empty screen comes inside attach, before the exit.
    let controls = first.controls();
    assert!(is_hello(&controls[0]));
    assert_eq!(
        controls.last(),
        Some(&ServerMessage::Exit { code: Some(0) })
    );
    assert!(!h.service.is_open("T-0001", "1"));
    let (_, again) = client();
    h.attach(&again);
    assert_eq!(h.ptys().len(), 2);
}

#[test]
fn closes_the_shell_of_a_task() {
    let h = setup();
    let (first, first_ref) = client();
    h.attach(&first_ref);

    h.service.close("T-0001");
    h.pump();

    assert!(h.pty(0).killed());
    assert_eq!(
        first.controls().last(),
        Some(&ServerMessage::Exit { code: None })
    );
    assert!(!h.service.is_open("T-0001", "1"));
}

#[test]
fn refuses_a_task_without_a_workspace() {
    let h = setup();
    let (_, one) = client();
    let refused = h
        .service
        .attach("T-0009", one, SIZE, "1", AttachOptions::default());
    assert!(matches!(refused, Err(TerminalError::NoWorkspace(_))));
    assert!(h.ptys().is_empty());
}

#[test]
fn refuses_a_new_shell_above_the_limit() {
    // T-0210: 30 agents of the default pipeline plus plain terminals.
    assert_eq!(MAX_SESSIONS, 64);
    let h = setup();
    // From T-0101 on, away from T-0009, which has no workspace here.
    for number in 1..=MAX_SESSIONS {
        let (_, one) = client();
        h.attach_to(
            &format!("T-{:04}", 100 + number),
            &one,
            SIZE,
            "1",
            AttachOptions::default(),
        );
    }
    let (_, one) = client();
    let refused = h
        .service
        .attach("T-0999", one, SIZE, "1", AttachOptions::default());
    assert_eq!(refused.err(), Some(TerminalError::Limit(MAX_SESSIONS)));
}
