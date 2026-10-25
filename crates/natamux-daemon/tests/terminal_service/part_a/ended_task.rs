//! `describe('shells of an ended task')`.

use std::sync::Arc;

use natamux_daemon::terminals::limits::{ENDED_IDLE_MS, MAIN_TERMINAL};
use natamux_pty::Signal;

use super::util::{attach_at, signals};
use crate::support::{Harness, SIZE, SetupOptions, setup_with};

fn ended_setup() -> Harness {
    setup_with(SetupOptions {
        is_ended: Some(Arc::new(|task_id: &str| task_id == "T-0002")),
        ..SetupOptions::default()
    })
}

#[test]
fn ends_an_idle_shell_a_while_after_the_last_client_leaves() {
    let h = ended_setup();
    let (_, _, detach_one) = attach_at(&h, "T-0002", SIZE, "1");
    let (_, _, detach_two) = attach_at(&h, "T-0002", SIZE, "1");

    detach_one.detach();
    h.advance(ENDED_IDLE_MS);
    assert!(h.service.is_open("T-0002", "1"));

    detach_two.detach();
    h.advance(ENDED_IDLE_MS - 1);
    assert!(h.service.is_open("T-0002", "1"));
    h.advance(1);
    assert!(!h.service.is_open("T-0002", "1"));
    assert_eq!(signals(&h, 0), [Signal::Hup]);
}

#[test]
fn keeps_the_shell_for_a_client_that_comes_back_in_time() {
    let h = ended_setup();
    attach_at(&h, "T-0002", SIZE, "1").2.detach();

    h.advance(ENDED_IDLE_MS - 1);
    attach_at(&h, "T-0002", SIZE, "1");
    h.advance(ENDED_IDLE_MS * 2);

    assert!(h.service.is_open("T-0002", "1"));
    assert_eq!(h.ptys().len(), 1);
}

#[test]
fn keeps_a_shell_that_runs_a_program_and_ends_it_once_the_program_is_done() {
    let h = ended_setup();
    attach_at(&h, "T-0002", SIZE, "1").2.detach();
    h.pty(0).set_program(Some("node"));

    h.advance(ENDED_IDLE_MS * 3);
    assert!(h.service.is_open("T-0002", "1"));

    h.pty(0).set_program(Some("/bin/zsh"));
    h.advance(ENDED_IDLE_MS);
    assert!(!h.service.is_open("T-0002", "1"));
}

#[test]
fn keeps_the_shells_of_a_task_that_is_still_open() {
    let h = ended_setup();
    attach_at(&h, "T-0001", SIZE, "1").2.detach();

    h.advance(ENDED_IDLE_MS * 3);
    assert!(h.service.is_open("T-0001", "1"));
}

#[test]
fn ends_a_kept_shell_once_its_program_is_done_also_when_no_client_ever_came() {
    let h = ended_setup();
    h.service
        .run("T-0002", "pnpm dev", MAIN_TERMINAL, SIZE)
        .unwrap();
    h.pty(0).set_program(Some("node"));

    h.service.end_when_idle("T-0002", MAIN_TERMINAL);
    h.advance(ENDED_IDLE_MS * 3);
    assert!(h.service.is_open("T-0002", "1"));

    h.pty(0).set_program(Some("zsh"));
    h.advance(ENDED_IDLE_MS);
    assert!(!h.service.is_open("T-0002", "1"));
}
