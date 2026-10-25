//! How much history a terminal keeps above its screen (`scrollbackLines` in
//! `packages/core/src/terminals/terminal.ts`): the dashboard, and the screen the daemon keeps for
//! each shell. Like Ghostty, the limit is memory, not lines: a wide terminal keeps fewer rows.

pub const TERMINAL_HISTORY_BYTES: usize = 10_000_000;
/// What xterm.js holds for one cell.
const BYTES_PER_CELL: usize = 16;
pub const TERMINAL_SCROLLBACK_MIN_LINES: usize = 1000;
pub const TERMINAL_SCROLLBACK_MAX_LINES: usize = 10_000;

/// The rows a terminal of this width keeps above its screen.
pub fn scrollback_lines(cols: u16) -> usize {
    let fit = TERMINAL_HISTORY_BYTES / (usize::from(cols.max(1)) * BYTES_PER_CELL);
    fit.clamp(TERMINAL_SCROLLBACK_MIN_LINES, TERMINAL_SCROLLBACK_MAX_LINES)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_terminal_keeps_fewer_rows_within_the_limits() {
        assert_eq!(scrollback_lines(1), TERMINAL_SCROLLBACK_MAX_LINES);
        assert_eq!(scrollback_lines(80), 7812);
        assert_eq!(scrollback_lines(200), 3125);
        assert_eq!(scrollback_lines(1000), TERMINAL_SCROLLBACK_MIN_LINES);
        assert_eq!(scrollback_lines(0), TERMINAL_SCROLLBACK_MAX_LINES);
    }
}
