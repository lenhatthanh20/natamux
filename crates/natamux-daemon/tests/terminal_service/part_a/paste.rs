//! `describe('paste')`.

use natamux_daemon::terminals::limits::{ENTER_AFTER_PASTE_MS, INPUT_GAP_MS};
use natamux_daemon::terminals::types::TerminalError;

use super::util::attach_one;
use crate::support::{Harness, Size, setup};

fn pasting() -> Harness {
    let h = setup();
    attach_one(&h);
    h
}

fn paste(h: &Harness, text: &str) {
    h.service.paste("T-0001", text, "1").unwrap();
}

fn input(h: &Harness, data: &str) {
    h.service.input("T-0001", "1", None, data);
}

fn wrapped(text: &str) -> String {
    format!("\x1b[200~{text}\x1b[201~")
}

#[test]
fn pastes_text_into_the_first_shell_then_presses_enter_apart_a_moment_later() {
    let h = pasting();

    paste(&h, "Rename it.\nThen test.");
    assert_eq!(h.pty(0).written(), [wrapped("Rename it.\nThen test.")]);

    h.advance(ENTER_AFTER_PASTE_MS - 1);
    assert_eq!(h.pty(0).written().len(), 1);
    h.advance(1);
    assert_eq!(
        h.pty(0).written(),
        [wrapped("Rename it.\nThen test."), "\r".into()]
    );
}

#[test]
fn keeps_input_typed_meanwhile_after_the_enter_of_the_paste() {
    let h = pasting();

    paste(&h, "Run the tests.");
    input(&h, "x");
    h.advance(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS);

    assert_eq!(
        h.pty(0).written(),
        [wrapped("Run the tests."), "\r".into(), "x".into()]
    );
}

#[test]
fn submits_each_of_two_pastes_on_its_own() {
    let h = pasting();

    paste(&h, "One");
    paste(&h, "Two");
    h.advance((ENTER_AFTER_PASTE_MS + INPUT_GAP_MS) * 2);

    let expected = [wrapped("One"), "\r".into(), wrapped("Two"), "\r".into()];
    assert_eq!(h.pty(0).written(), expected);
}

#[test]
fn does_not_slow_down_the_typed_enter_of_a_command_line() {
    let h = pasting();
    input(&h, "ls");
    input(&h, "\r");
    assert_eq!(h.pty(0).written(), ["ls", "\r"]);
}

#[test]
fn keeps_text_with_lf_line_breaks_tabs_or_no_text_in_the_paste_and_presses_enter_once() {
    for text in ["line 1\nline 2\n\nline 4", "line 1\n", "a\tb", ""] {
        let h = pasting();

        paste(&h, text);
        h.advance(ENTER_AFTER_PASTE_MS * 10);

        assert_eq!(h.pty(0).written(), [wrapped(text), "\r".into()], "{text:?}");
    }
}

// An escape in the text can end the paste early, and then the agent takes its Enter as a line
// break, not as a submit.
#[test]
fn takes_crlf_a_lone_cr_ends_of_a_paste_colors_and_control_bytes_out_of_the_paste() {
    let cases = [
        ("line 1\r\nline 2", "line 1\nline 2"),
        ("line 1\rline 2", "line 1\nline 2"),
        ("a\x1b[201~b", "ab"),
        ("\x1b[31mFAIL\x1b[0m", "FAIL"),
        ("a\x00\x07\x08\x7fb", "ab"),
        ("a\x1b[1m\tb\r\n\x00c", "a\tb\nc"),
        ("\x1b[0m\x00\x07", ""),
    ];
    for (text, pasted) in cases {
        let h = pasting();

        paste(&h, text);
        h.advance(ENTER_AFTER_PASTE_MS * 10);

        assert_eq!(
            h.pty(0).written(),
            [wrapped(pasted), "\r".into()],
            "{text:?}"
        );
    }
}

#[test]
fn drops_the_enter_of_a_paste_when_the_shell_is_closed_before_it() {
    let h = pasting();

    paste(&h, "hello");
    h.service.close_one("T-0001", "1");

    h.advance(ENTER_AFTER_PASTE_MS * 2);
    assert_eq!(h.pty(0).written(), [wrapped("hello")]);
}

#[test]
fn drops_the_enter_of_a_paste_when_the_shell_ends_before_it() {
    let h = pasting();

    paste(&h, "hello");
    h.pty(0).exit(Some(0));

    h.advance(ENTER_AFTER_PASTE_MS * 2);
    assert_eq!(h.pty(0).written(), [wrapped("hello")]);
}

#[test]
fn refuses_a_task_without_an_open_shell() {
    // Rust: the scheduler has no timer count, so "starts no timer" is not checked.
    let h = setup();
    let refused = h.service.paste("T-0001", "x", "1");
    assert!(matches!(refused, Err(TerminalError::NoTerminal(_))));
}

#[test]
fn keeps_several_keys_typed_meanwhile_after_the_enter_in_order() {
    let h = pasting();

    paste(&h, "prompt");
    for key in ["a", "b", "c"] {
        input(&h, key);
    }
    h.advance(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS);

    let expected = [
        wrapped("prompt"),
        "\r".into(),
        "a".into(),
        "b".into(),
        "c".into(),
    ];
    assert_eq!(h.pty(0).written(), expected);
}

#[test]
fn submits_a_paste_with_its_own_enter_when_an_enter_is_typed_meanwhile() {
    let h = pasting();

    paste(&h, "prompt");
    input(&h, "\r");
    h.advance(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS);

    assert_eq!(
        h.pty(0).written(),
        [wrapped("prompt"), "\r".into(), "\r".into()]
    );
}

#[test]
fn writes_input_typed_before_a_paste_first_unchanged() {
    let h = pasting();

    input(&h, "x");
    paste(&h, "prompt");
    h.advance(ENTER_AFTER_PASTE_MS);

    assert_eq!(
        h.pty(0).written(),
        ["x".into(), wrapped("prompt"), "\r".into()]
    );
}

#[test]
fn keeps_the_order_when_the_shell_is_resized_before_the_enter() {
    let h = pasting();

    paste(&h, "prompt");
    h.service.resize(
        "T-0001",
        "1",
        None,
        Size {
            cols: 100,
            rows: 30,
        },
    );
    input(&h, "x");
    h.advance(ENTER_AFTER_PASTE_MS + INPUT_GAP_MS);

    assert_eq!(
        h.pty(0).written(),
        [wrapped("prompt"), "\r".into(), "x".into()]
    );
}
