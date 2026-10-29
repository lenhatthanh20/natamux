//! Rules of the diff view that do not need the editor: a hunk of the buffer diff as a
//! `LineChange`, the empty rows that keep the two sides of a side by side diff level, and the
//! width rules of the Changes tab (`ChangesPanel.tsx`).

use std::cmp::Ordering;
use std::ops::Range;

use super::change_rules::{LineChange, Side};

/// `WIDE_FROM`: the file panel sits beside the diff from this main width; below, a picker.
pub const WIDE_FROM: f32 = 720.;
/// `SIDE_BY_SIDE_FROM`: the diff column needs this width for two sides.
pub const SIDE_BY_SIDE_FROM: f32 = 720.;
/// `FULL_LABELS_FROM`: the layout toggle shows its labels from this main width.
pub const FULL_LABELS_FROM: f32 = 960.;
/// The per-file `+A −R` shows from this file panel width (15rem).
pub const COUNTS_FROM: f32 = 240.;
pub const NEEDS_WIDER: &str = "Needs a wider window";

/// One hunk, as rows of both sides: 0-based, the end not included. An empty range is a hunk
/// with no lines on that side; it sits before the row at its start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HunkRows {
    pub old: Range<u32>,
    pub new: Range<u32>,
}

/// Monaco's shape of a change: 1-based lines, and an end of 0 with the line it follows as the
/// start when a side has no lines.
pub fn line_change(hunk: &HunkRows) -> LineChange {
    let side = |rows: &Range<u32>| -> (u32, u32) {
        if rows.is_empty() {
            (rows.start, 0)
        } else {
            (rows.start + 1, rows.end)
        }
    };
    let (original_start, original_end) = side(&hunk.old);
    let (modified_start, modified_end) = side(&hunk.new);
    LineChange {
        original_start,
        original_end,
        modified_start,
        modified_end,
    }
}

/// Empty rows to add on one side, above `before_row` of that side (or below its last row when
/// `before_row` is past the end), so the rows after a hunk line up on both sides.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Spacer {
    pub side: Side,
    pub before_row: u32,
    pub rows: u32,
}

/// The spacers of a side by side diff: the shorter side of each hunk gets the difference.
pub fn spacers(hunks: &[HunkRows]) -> Vec<Spacer> {
    hunks
        .iter()
        .filter_map(|hunk| {
            let old = hunk.old.end - hunk.old.start;
            let new = hunk.new.end - hunk.new.start;
            if old < new {
                Some(Spacer {
                    side: Side::Old,
                    before_row: hunk.old.end,
                    rows: new - old,
                })
            } else if new < old {
                Some(Spacer {
                    side: Side::New,
                    before_row: hunk.new.end,
                    rows: old - new,
                })
            } else {
                None
            }
        })
        .collect()
}

/// Empty rows at the end of the side whose rows after the last hunk are fewer: a text with a final
/// line break has one more (empty) last row than a text without. `old_rows` and `new_rows` count
/// all rows of each side.
pub fn end_spacer(hunks: &[HunkRows], old_rows: u32, new_rows: u32) -> Option<Spacer> {
    let (old_end, new_end) = hunks
        .last()
        .map_or((0, 0), |hunk| (hunk.old.end, hunk.new.end));
    let old_tail = old_rows.saturating_sub(old_end);
    let new_tail = new_rows.saturating_sub(new_end);
    match old_tail.cmp(&new_tail) {
        Ordering::Less => Some(Spacer {
            side: Side::Old,
            before_row: old_rows,
            rows: new_tail - old_tail,
        }),
        Ordering::Greater => Some(Spacer {
            side: Side::New,
            before_row: new_rows,
            rows: old_tail - new_tail,
        }),
        Ordering::Equal => None,
    }
}

/// Side by side is on when the user wants it and the diff column has room; the stored choice
/// stays either way. `Some(reason)` when it is off for want of room.
pub fn side_by_side(wanted: bool, diff_width: f32) -> (bool, Option<&'static str>) {
    if diff_width < SIDE_BY_SIDE_FROM {
        (false, Some(NEEDS_WIDER))
    } else {
        (wanted, None)
    }
}

/// The file panel width: the stored one, but the diff keeps at least half of main.
pub fn files_width(stored: u32, main_width: f32) -> f32 {
    (stored as f32).min((main_width / 2.).max(200.))
}

/// What a new `show` of the diff does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffRefresh {
    /// Nothing changed.
    Keep,
    /// A newer read of the same file: the texts change inside the editors, so the cursor, the
    /// selection and the scroll stay.
    Update,
    /// Another file, or other options: new editors.
    Rebuild,
}

pub fn diff_refresh(same_file: bool, same_read: bool, same_options: bool) -> DiffRefresh {
    match (same_file, same_read, same_options) {
        (true, true, true) => DiffRefresh::Keep,
        (true, false, true) => DiffRefresh::Update,
        _ => DiffRefresh::Rebuild,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hunk(old: Range<u32>, new: Range<u32>) -> HunkRows {
        HunkRows { old, new }
    }

    #[test]
    fn a_changed_line_covers_one_line_on_each_side() {
        let change = line_change(&hunk(3..4, 3..5));
        assert_eq!(
            change,
            LineChange {
                original_start: 4,
                original_end: 4,
                modified_start: 4,
                modified_end: 5,
            }
        );
    }

    #[test]
    fn an_added_block_has_no_old_lines_and_follows_its_old_line() {
        let change = line_change(&hunk(2..2, 2..3));
        assert_eq!((change.original_start, change.original_end), (2, 0));
        assert_eq!((change.modified_start, change.modified_end), (3, 3));
    }

    #[test]
    fn a_deletion_at_the_top_follows_line_0() {
        let change = line_change(&hunk(0..2, 0..0));
        assert_eq!((change.modified_start, change.modified_end), (0, 0));
        assert_eq!((change.original_start, change.original_end), (1, 2));
    }

    #[test]
    fn the_shorter_side_of_a_hunk_gets_the_empty_rows() {
        let hunks = [
            hunk(2..2, 2..3),
            hunk(4..5, 5..7),
            hunk(8..10, 10..11),
            hunk(12..13, 13..14),
        ];
        assert_eq!(
            spacers(&hunks),
            [
                Spacer {
                    side: Side::Old,
                    before_row: 2,
                    rows: 1,
                },
                Spacer {
                    side: Side::Old,
                    before_row: 5,
                    rows: 1,
                },
                Spacer {
                    side: Side::New,
                    before_row: 11,
                    rows: 1,
                },
            ]
        );
    }

    #[test]
    fn an_end_of_file_hunk_gets_its_empty_rows_on_the_shorter_side() {
        // T-0230: lines deleted at the end, lines added at the end, and the last line changed
        // when the final line break was dropped.
        let deleted = spacers(&[hunk(280..427, 280..280)]);
        let added = spacers(&[hunk(280..280, 280..427)]);
        let last_changed = spacers(&[hunk(279..427, 279..280)]);
        assert_eq!(
            deleted,
            [Spacer {
                side: Side::New,
                before_row: 280,
                rows: 147,
            }]
        );
        assert_eq!(
            added,
            [Spacer {
                side: Side::Old,
                before_row: 280,
                rows: 147,
            }]
        );
        assert_eq!(
            last_changed,
            [Spacer {
                side: Side::New,
                before_row: 280,
                rows: 147,
            }]
        );
    }

    #[test]
    fn the_side_with_fewer_rows_after_the_last_hunk_gets_one_at_the_end() {
        // Only the old text has a final line break, so only it has an empty last row.
        let hunks = [hunk(277..427, 277..280)];
        assert_eq!(
            end_spacer(&hunks, 428, 280),
            Some(Spacer {
                side: Side::New,
                before_row: 280,
                rows: 1,
            })
        );
        assert_eq!(
            end_spacer(&hunks, 427, 281),
            Some(Spacer {
                side: Side::Old,
                before_row: 427,
                rows: 1,
            })
        );
        assert_eq!(end_spacer(&hunks, 428, 281), None);
        assert_eq!(end_spacer(&[], 100, 100), None);
    }

    #[test]
    fn side_by_side_needs_a_wide_diff_column_and_keeps_the_choice() {
        assert_eq!(side_by_side(true, 719.), (false, Some(NEEDS_WIDER)));
        assert_eq!(side_by_side(false, 719.), (false, Some(NEEDS_WIDER)));
        assert_eq!(side_by_side(true, 720.), (true, None));
        assert_eq!(side_by_side(false, 900.), (false, None));
    }

    #[test]
    fn the_file_panel_leaves_the_diff_at_least_half() {
        assert_eq!(files_width(260, 1200.), 260.);
        assert_eq!(files_width(420, 760.), 380.);
        assert_eq!(files_width(260, 300.), 200.);
    }

    #[test]
    fn a_newer_read_of_the_same_file_updates_the_editors_in_place() {
        // Regression (T-0150): a changed read made new editors, so the cursor was lost.
        assert_eq!(diff_refresh(true, false, true), DiffRefresh::Update);
        assert_eq!(diff_refresh(true, true, true), DiffRefresh::Keep);
    }

    #[test]
    fn another_file_or_other_options_build_new_editors() {
        assert_eq!(diff_refresh(false, false, true), DiffRefresh::Rebuild);
        assert_eq!(diff_refresh(false, true, true), DiffRefresh::Rebuild);
        assert_eq!(diff_refresh(true, true, false), DiffRefresh::Rebuild);
        assert_eq!(diff_refresh(true, false, false), DiffRefresh::Rebuild);
    }
}
