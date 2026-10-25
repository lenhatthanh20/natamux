//! `describe('terminal tickets')`.

use natamux_daemon::terminals::limits::TICKET_TTL_MS;
use natamux_daemon::terminals::types::TerminalError;

use crate::support::setup;

#[test]
fn works_once_and_only_for_its_task() {
    let h = setup();
    let ticket = h.service.issue_ticket("T-0001").unwrap();

    assert!(!h.service.redeem_ticket(&ticket, "T-0002"));
    assert!(!h.service.redeem_ticket(&ticket, "T-0001"));

    let another = h.service.issue_ticket("T-0001").unwrap();
    assert!(h.service.redeem_ticket(&another, "T-0001"));
    assert!(!h.service.redeem_ticket(&another, "T-0001"));
}

#[test]
fn expires() {
    let h = setup();
    let ticket = h.service.issue_ticket("T-0001").unwrap();

    h.scheduler.advance(TICKET_TTL_MS);
    assert!(!h.service.redeem_ticket(&ticket, "T-0001"));
}

#[test]
fn is_not_given_for_a_task_without_a_workspace() {
    let h = setup();
    assert!(matches!(
        h.service.issue_ticket("T-0009"),
        Err(TerminalError::NoWorkspace(_))
    ));
}

#[test]
fn refuses_an_unknown_ticket() {
    let h = setup();
    h.service.issue_ticket("T-0001").unwrap();
    assert!(!h.service.redeem_ticket("nope", "T-0001"));
}
