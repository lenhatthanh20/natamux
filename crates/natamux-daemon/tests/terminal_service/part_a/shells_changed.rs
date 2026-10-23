//! `describe('shells.changed')`.

use super::util::attach_at;
use crate::support::{SIZE, setup, shells_changed};

#[test]
fn sends_one_message_per_shell_when_all_shells_of_a_task_close_each_for_its_task() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    attach_at(&h, "T-0001", SIZE, "2");
    attach_at(&h, "T-0002", SIZE, "1");
    let changed = shells_changed("T-0001");
    assert_eq!(
        h.messages(),
        [changed.clone(), changed.clone(), shells_changed("T-0002")]
    );

    h.service.close("T-0001");
    h.pump();

    assert_eq!(h.messages()[3..], [changed.clone(), changed]);
}

#[test]
fn sends_nothing_more_when_a_closed_shell_ends_later() {
    let h = setup();
    attach_at(&h, "T-0001", SIZE, "1");
    // It ignores SIGHUP, so it ends after close_one.
    h.pty(0).state().stubborn = true;

    h.service.close_one("T-0001", "1");
    h.pump();
    assert_eq!(h.messages().len(), 2);
    h.pty(0).exit(Some(0));

    assert_eq!(h.messages().len(), 2);
}
