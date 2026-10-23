//! `describe('input with DEL')`.

use natamux_daemon::terminals::limits::INPUT_GAP_MS;

use super::util::attach_one;
use crate::support::{Harness, setup};

fn typing() -> Harness {
    let h = setup();
    attach_one(&h);
    h
}

#[test]
fn sends_the_letter_that_a_vietnamese_input_method_types_after_a_del_a_moment_later() {
    let h = typing();
    for data in ["a", "\x7f", "á"] {
        h.service.input("T-0001", "1", None, data);
    }
    assert_eq!(h.pty(0).written(), ["a"]);

    h.advance(INPUT_GAP_MS);
    assert_eq!(h.pty(0).written(), ["a", "\x7f"]);
    h.advance(INPUT_GAP_MS - 1);
    assert_eq!(h.pty(0).written(), ["a", "\x7f"]);
    h.advance(1);
    assert_eq!(h.pty(0).written(), ["a", "\x7f", "á"]);
}

#[test]
fn splits_one_piece_of_input_at_its_dels_and_keeps_the_order() {
    let h = typing();
    h.service.input("T-0001", "1", None, "vie\x7f\x7fiê");
    h.service.input("T-0001", "1", None, "t");
    h.advance(INPUT_GAP_MS * 2);
    assert_eq!(h.pty(0).written(), ["vie", "\x7f\x7f", "iê", "t"]);
}

#[test]
fn does_not_slow_down_typing_without_del_or_holding_backspace() {
    let h = typing();
    for key in ["l", "s", "\r"] {
        h.service.input("T-0001", "1", None, key);
    }
    h.advance(INPUT_GAP_MS);
    for key in ["\x7f", "\x7f", "\x7f"] {
        h.service.input("T-0001", "1", None, key);
    }
    assert_eq!(h.pty(0).written(), ["l", "s", "\r", "\x7f", "\x7f", "\x7f"]);
}

#[test]
fn drops_input_that_waits_when_the_shell_is_closed() {
    let h = typing();
    h.service.input("T-0001", "1", None, "\x7f");
    h.service.input("T-0001", "1", None, "á");
    h.service.close_one("T-0001", "1");
    h.advance(INPUT_GAP_MS * 2);
    assert_eq!(h.pty(0).written(), ["\x7f"]);
}
