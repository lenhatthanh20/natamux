//! A catch-up snapshot holds the last `CATCH_UP_HISTORY_ROWS` history rows; a first attach holds
//! all of them.

use natamux_daemon::terminals::service::AttachOptions;
use natamux_proto::terminals::history::CATCH_UP_HISTORY_ROWS;

use super::util::{MAIN, TASK};
use crate::support::{SIZE, client, hello_id, numbered, numbered_lines, setup};

const LINES: usize = 3000;

#[test]
fn a_first_attach_gets_the_full_history_and_a_missed_offset_only_the_last_rows() {
    let h = setup();
    let (first, first_ref) = client();
    let detach = h.attach(&first_ref);
    h.pty(0).emit(numbered(LINES));
    h.settled(TASK, MAIN);
    detach.detach();
    let shell_id = first
        .controls()
        .iter()
        .find_map(hello_id)
        .unwrap_or_default();
    let all = numbered_lines(LINES);

    // No resume offset: nothing is held yet, so all of the history comes.
    let (fresh, fresh_ref) = client();
    h.attach(&fresh_ref);
    h.settled(TASK, MAIN);
    assert_eq!(fresh.screen(SIZE), all);

    // An offset the ring never had: a catch-up, so the last rows only.
    let (back, back_ref) = client();
    let options = AttachOptions {
        shell_id: Some(shell_id.clone()),
        offset: Some(u64::MAX / 2),
        ..AttachOptions::default()
    };
    h.attach_to(TASK, &back_ref, SIZE, MAIN, options);
    h.settled(TASK, MAIN);
    let rows = back.screen(SIZE);
    let kept = CATCH_UP_HISTORY_ROWS + usize::from(SIZE.rows) - 1;
    assert_eq!(rows, all[LINES - kept..]);

    // A resume with no offset is a first attach too: the full history.
    let (blank, blank_ref) = client();
    let options = AttachOptions {
        shell_id: Some(shell_id),
        offset: None,
        ..AttachOptions::default()
    };
    h.attach_to(TASK, &blank_ref, SIZE, MAIN, options);
    h.settled(TASK, MAIN);
    assert_eq!(blank.screen(SIZE), all);
}
