//! `describe('several terminals of a task')`.

use super::util::{attach_at, attach_one};
use crate::support::{SIZE, setup};

#[test]
fn keeps_one_shell_per_terminal_name_each_in_the_workspace() {
    let h = setup();
    let (first, _, _) = attach_one(&h);
    let (second, _, _) = attach_at(&h, "T-0001", SIZE, "2");

    h.service.input("T-0001", "2", None, "a");
    h.settled("T-0001", "2");

    assert_eq!(h.ptys().len(), 2);
    assert_eq!(h.pty(1).state().options.cwd.to_str(), Some("/work/T-0001"));
    assert_eq!(h.pty(1).written(), ["a"]);
    assert!(first.bytes().is_empty());
    assert_eq!(second.output(), "a");
    assert_eq!(h.service.list("T-0001"), ["1", "2"]);
}

#[test]
fn lists_terminals_in_number_order_and_only_those_of_the_task() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "10");
    attach_at(&h, "T-0001", SIZE, "2");
    attach_at(&h, "T-0002", SIZE, "1");

    assert_eq!(h.service.list("T-0001"), ["2", "10"]);
}

#[test]
fn closes_one_terminal_or_all_of_a_task() {
    let h = setup();
    attach_one(&h);
    attach_at(&h, "T-0001", SIZE, "2");
    attach_at(&h, "T-0002", SIZE, "1");

    h.service.close_one("T-0001", "2");
    assert_eq!(h.service.list("T-0001"), ["1"]);

    h.service.close("T-0001");
    assert!(h.service.list("T-0001").is_empty());
    let killed: Vec<bool> = h.ptys().iter().map(|pty| pty.killed()).collect();
    assert_eq!(killed, [true, true, false]);
}

#[test]
fn runs_an_agent_in_the_first_terminal() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "2");

    h.service.run("T-0001", "claude", "1", SIZE).unwrap();

    assert_eq!(h.ptys().len(), 2);
    assert_eq!(h.pty(1).written(), ["claude\r"]);
    assert!(h.service.is_open("T-0001", "1"));
}
