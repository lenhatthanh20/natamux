//! The size rules of an app terminal: when a new grid size goes to the shell, and how much
//! history the terminal keeps at its width.

use natamux_proto::terminals::history::scrollback_lines;

/// The history of a terminal made before its pane has a width. At its first size it is built
/// again with the history of that width, before any output comes.
pub const FIRST_HISTORY_COLS: u16 = 120;

/// The rows of history a terminal `cols` wide keeps: the daemon's rule, so the app keeps the
/// lines the daemon does. None for a grid with no columns.
pub fn history_lines(cols: u16) -> Option<usize> {
    (cols > 0).then(|| scrollback_lines(cols))
}

/// What a size check does.
#[derive(Debug, PartialEq, Eq)]
pub enum SizeStep {
    /// The first size: the shell starts at it, at once.
    Connect((u16, u16)),
    /// A new size that settled.
    Resize((u16, u16)),
}

/// What a size check does with the grid size `now`. A grid with no columns or rows is not laid
/// out yet. A later new size goes to the shell only once two checks in a row saw it (the web's
/// `FIT_DELAY_MS`): each resize makes a TUI repaint, and a layout change passes through sizes
/// (T-0173).
pub fn size_step(
    has_handle: bool,
    seen: Option<(u16, u16)>,
    now: (u16, u16),
    sent: (u16, u16),
) -> Option<SizeStep> {
    if now.0 == 0 || now.1 == 0 || now == sent {
        return None;
    }
    if !has_handle {
        return Some(SizeStep::Connect(now));
    }
    (seen == Some(now)).then_some(SizeStep::Resize(now))
}

/// After a look at the grid size `now`: a size that is not the shell's yet is looked at again
/// once it held. A grid with no columns or rows waits for its next layout instead.
pub fn waits(now: (u16, u16), sent: (u16, u16)) -> bool {
    now.0 > 0 && now.1 > 0 && now != sent
}

#[cfg(test)]
mod tests {
    use natamux_proto::terminals::history::scrollback_lines;

    use super::{SizeStep, history_lines, size_step, waits};

    #[test]
    fn a_size_the_shell_does_not_have_is_looked_at_again() {
        assert!(waits((81, 24), (80, 24)));
        assert!(!waits((80, 24), (80, 24)));
    }

    #[test]
    fn a_grid_with_no_columns_or_rows_waits_for_its_next_layout() {
        assert!(!waits((0, 24), (80, 24)));
        assert!(!waits((80, 0), (80, 24)));
    }

    // The settle rule of T-0173 without a timer per terminal: a layout looks at once, and only a
    // size that is not sent yet starts one wait of SIZE_SETTLE.
    #[test]
    fn a_layout_change_resizes_after_one_wait_and_then_waits_no_more() {
        let sent = (80, 24);
        let seen = Some((80, 24));
        let now = (100, 30);
        assert_eq!(size_step(true, seen, now, sent), None);
        assert!(waits(now, sent));
        assert_eq!(
            size_step(true, Some(now), now, sent),
            Some(SizeStep::Resize(now))
        );
        assert!(!waits(now, now));
    }

    #[test]
    fn keeps_the_history_of_the_daemons_rule_at_its_own_width() {
        for cols in [1, 40, 80, 120, 200, 1000] {
            assert_eq!(history_lines(cols), Some(scrollback_lines(cols)));
        }
        assert_eq!(history_lines(0), None);
        // A wider terminal keeps fewer rows, so the width at connect sets the limit.
        assert!(history_lines(200) < history_lines(80));
        assert!(history_lines(120).is_some_and(|lines| lines < 10_000));
    }

    #[test]
    fn the_first_size_connects_at_once() {
        assert_eq!(
            size_step(false, None, (80, 24), (0, 0)),
            Some(SizeStep::Connect((80, 24)))
        );
        // A connect that failed is not tried again at the same size.
        assert_eq!(size_step(false, Some((80, 24)), (80, 24), (80, 24)), None);
    }

    #[test]
    fn a_new_size_waits_for_a_second_check() {
        assert_eq!(size_step(true, Some((80, 24)), (81, 24), (80, 24)), None);
        assert_eq!(size_step(true, None, (81, 24), (80, 24)), None);
    }

    #[test]
    fn a_size_seen_twice_goes_out_once() {
        assert_eq!(
            size_step(true, Some((81, 24)), (81, 24), (80, 24)),
            Some(SizeStep::Resize((81, 24)))
        );
        assert_eq!(size_step(true, Some((81, 24)), (81, 24), (81, 24)), None);
    }

    // T-0173: a layout that passes through a size and comes back sends nothing.
    #[test]
    fn a_size_that_comes_back_sends_nothing() {
        assert_eq!(size_step(true, Some((81, 24)), (80, 24), (80, 24)), None);
        assert_eq!(size_step(true, Some((80, 24)), (80, 24), (80, 24)), None);
    }

    #[test]
    fn three_different_sizes_in_three_checks_send_nothing() {
        let mut seen = Some((80, 24));
        for now in [(81, 24), (82, 24), (83, 24)] {
            assert_eq!(size_step(true, seen, now, (80, 24)), None);
            seen = Some(now);
        }
    }

    #[test]
    fn a_grid_with_no_columns_or_rows_never_connects_or_resizes() {
        assert_eq!(size_step(false, None, (0, 24), (0, 0)), None);
        assert_eq!(size_step(false, Some((80, 0)), (80, 0), (0, 0)), None);
        assert_eq!(size_step(true, Some((0, 0)), (0, 0), (80, 24)), None);
    }
}
