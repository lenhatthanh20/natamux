//! `describe('the last screen of a shell')`.

use super::util::attach_one;
use crate::support::setup;

#[test]
fn reads_the_last_lines_as_plain_text_without_the_empty_rows_under_them() {
    let h = setup();
    attach_one(&h);
    h.pty(0).emit("one\r\n\x1b[31mtwo\x1b[0m\r\nthree\r\n");
    h.settled("T-0001", "1");

    assert_eq!(
        h.service.last_screen("T-0001", "1", 2).as_deref(),
        Some("two\nthree")
    );
    assert_eq!(
        h.service.last_screen("T-0001", "1", 10).as_deref(),
        Some("one\ntwo\nthree")
    );
}

#[test]
fn reads_nothing_for_a_shell_that_is_not_open() {
    let h = setup();

    assert_eq!(h.service.last_screen("T-0001", "7", 10), None);
}
