//! `describe('closing a shell')`, the cases that wait for the shells to end.

use natamux_daemon::terminals::limits::{EXIT_WAIT_MS, KILL_GRACE_MS};
use natamux_daemon::terminals::service::TerminalService;
use natamux_daemon::terminals::session_lifecycle::EndWait;
use natamux_pty::Signal;

use super::util::{attach_at, signals};
use crate::support::{SIZE, setup};

fn waits_for_an_orphan(close: impl Fn(&TerminalService) -> EndWait) {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "2");
    h.pty(1).state().orphan = true;

    let closing = close(&h.service);
    h.advance(KILL_GRACE_MS - 1);
    assert!(!closing.is_done());
    assert_eq!(signals(&h, 1), [Signal::Hup]);

    h.advance(1);
    assert!(closing.is_done());
    assert_eq!(signals(&h, 1), [Signal::Hup, Signal::Kill]);
    assert_eq!(signals(&h, 0), [Signal::Hup]);
}

#[test]
fn close_and_wait_waits_until_a_program_that_outlived_its_shell_is_killed() {
    waits_for_an_orphan(|service| service.close_and_wait("T-0001"));
}

#[test]
fn close_all_and_wait_waits_until_a_program_that_outlived_its_shell_is_killed() {
    waits_for_an_orphan(|service| service.close_all_and_wait());
}

#[test]
fn waits_until_the_shells_have_ended_at_most_until_a_stubborn_one_is_killed() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "2");
    h.pty(1).state().stubborn = true;

    let closing = h.service.close_and_wait("T-0001");
    h.advance(KILL_GRACE_MS - 1);
    assert!(!closing.is_done());

    h.advance(1);
    assert!(closing.is_done());
    assert!(h.pty(1).killed());
    assert!(h.service.list("T-0001").is_empty());
}

#[test]
fn at_shutdown_waits_for_the_shells_of_every_task_until_a_stubborn_one_is_killed() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0002", SIZE, "1");
    h.pty(1).state().stubborn = true;

    let closing = h.service.close_all_and_wait();
    h.advance(KILL_GRACE_MS - 1);
    assert!(!closing.is_done());
    assert!(!h.pty(1).killed());

    h.advance(1);
    assert!(closing.is_done());
    assert_eq!(signals(&h, 0), [Signal::Hup]);
    assert_eq!(signals(&h, 1), [Signal::Hup, Signal::Kill]);
    assert!(h.service.list("T-0001").is_empty());
    assert!(h.service.list("T-0002").is_empty());
}

#[test]
fn at_shutdown_ends_as_soon_as_the_last_shell_ends_on_sighup_with_no_sigkill() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0002", SIZE, "1");
    // It takes a while to end after SIGHUP: the fake ignores it.
    h.pty(1).state().stubborn = true;

    let closing = h.service.close_all_and_wait();
    h.advance(KILL_GRACE_MS / 2);
    assert!(!closing.is_done());

    h.pty(1).exit(Some(0));
    h.tick();
    assert!(closing.is_done());
    assert_eq!(signals(&h, 0), [Signal::Hup]);
    assert_eq!(signals(&h, 1), [Signal::Hup]);
    h.advance(KILL_GRACE_MS);
    assert_eq!(signals(&h, 1), [Signal::Hup]);
}

#[test]
fn at_shutdown_stops_waiting_for_a_shell_that_never_reports_its_end() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    // A killed fake reports no end again: it never does, as far as the service sees.
    h.pty(0).state().killed = true;

    let closing = h.service.close_all_and_wait();
    h.advance(KILL_GRACE_MS + EXIT_WAIT_MS - 1);
    assert!(!closing.is_done());
    assert_eq!(signals(&h, 0), [Signal::Hup, Signal::Kill]);

    h.advance(1);
    assert!(closing.is_done());
    assert!(h.service.list("T-0001").is_empty());
}

#[test]
fn at_shutdown_ends_at_once_when_no_shell_is_open_also_when_called_twice() {
    let h = setup();
    let (first, second) = (
        h.service.close_all_and_wait(),
        h.service.close_all_and_wait(),
    );
    assert!(first.is_done() && second.is_done());
}

#[test]
fn at_shutdown_ends_at_once_when_called_again_after_the_shells_are_closed() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    let closing = h.service.close_all_and_wait();
    h.pump();
    assert!(closing.is_done());

    assert!(h.service.close_all_and_wait().is_done());
    assert!(h.service.list("T-0001").is_empty());
}
