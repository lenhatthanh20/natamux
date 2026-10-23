//! `describe('closing a shell')`, the cases without a wait on the shells.

use std::sync::{Arc, Mutex};

use natamux_daemon::terminals::limits::KILL_GRACE_MS;
use natamux_proto::terminals::messages::ServerMessage;
use natamux_pty::Signal;

use super::util::{attach_at, signals};
use crate::support::{SIZE, SetupOptions, setup, setup_with};

#[test]
fn forgets_the_shell_at_once_and_a_new_shell_with_the_same_name_is_a_new_process() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    let (old, _, _) = attach_at(&h, "T-0001", SIZE, "2");
    h.pty(1).emit("old output");

    h.service.close_one("T-0001", "2");
    h.pump();
    assert_eq!(h.service.list("T-0001"), ["1"]);
    assert_eq!(
        old.controls().last(),
        Some(&ServerMessage::Exit { code: None })
    );

    let (fresh, _, _) = attach_at(&h, "T-0001", SIZE, "2");
    h.settled("T-0001", "2");
    assert_eq!(h.ptys().len(), 3);
    assert_ne!(h.pty(2).state().pid, h.pty(1).state().pid);
    assert!(fresh.bytes().is_empty());
}

#[test]
fn forgets_the_shell_even_when_its_process_is_slow_to_end_and_kills_it_after_the_grace_time() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "2");
    h.pty(0).state().stubborn = true;

    h.service.close_one("T-0001", "2");
    h.pump();
    assert!(!h.service.is_open("T-0001", "2"));
    assert_eq!(signals(&h, 0), [Signal::Hup]);
    assert!(!h.pty(0).killed());

    h.advance(KILL_GRACE_MS);
    assert_eq!(signals(&h, 0), [Signal::Hup, Signal::Kill]);
    assert!(h.pty(0).killed());
}

#[test]
fn does_not_send_sigkill_to_a_shell_that_ended_after_sighup() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "2");

    h.service.close_one("T-0001", "2");
    h.advance(KILL_GRACE_MS * 2);

    assert_eq!(signals(&h, 0), [Signal::Hup]);
}

#[test]
fn kills_a_program_that_ignored_sighup_and_outlived_its_shell_after_the_grace_time() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "2");
    h.pty(0).state().orphan = true;

    h.service.close_one("T-0001", "2");
    h.pump();
    assert!(h.pty(0).killed());
    h.advance(KILL_GRACE_MS - 1);
    assert_eq!(signals(&h, 0), [Signal::Hup]);

    h.advance(1);
    assert_eq!(signals(&h, 0), [Signal::Hup, Signal::Kill]);
    assert!(!h.pty(0).state().orphan);
}

#[test]
fn does_not_hand_the_output_of_a_closed_shell_to_anyone() {
    let heard = Arc::new(Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&heard);
    let h = setup_with(SetupOptions {
        on_output: Some(Arc::new(move |_, _, data: &[u8]| {
            sink.lock()
                .unwrap()
                .push(String::from_utf8_lossy(data).into_owned());
        })),
        ..SetupOptions::default()
    });
    attach_at(&h, "T-0001", SIZE, "2");
    h.service.close_one("T-0001", "2");

    h.pty(0).emit("late");

    assert!(heard.lock().unwrap().is_empty());
}
