//! What one side of the diff view shows (S6.2 part 2): the text, numbers, empty rows, markers
//! and words a side gets from the rows of the model.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::git::change_rules::Side;
use crate::git::diff_model::{self as model, diff, inline_rows};

fn numbers(side: &SideRows) -> Vec<Option<u32>> {
    side.lines.iter().map(|line| line.number).collect()
}

fn statuses(side: &SideRows) -> Vec<Status> {
    side.lines.iter().map(|line| line.status).collect()
}

/// The rows of an inline diff of `old` and `new`, on the working tree's side.
fn inline(old: &str, new: &str, collapse: bool) -> (FileDiff, SideRows) {
    let diff = diff(Some(old), new, &[]);
    let rows = inline_rows(&diff.hunk_rows(), diff.new_rows(), collapse);
    let side = side_rows(&diff, &rows, true);
    (diff, side)
}

/// The rows of one side of a side by side diff of `old` and `new`.
fn beside(old: &str, new: &str, side: Side) -> (FileDiff, SideRows) {
    let diff = diff(Some(old), new, &[]);
    let rows = model::side_rows(
        &diff.hunk_rows(),
        side,
        diff.old_rows(),
        diff.new_rows(),
        false,
    );
    let shown = side_rows(&diff, &rows, side == Side::New);
    (diff, shown)
}

#[test]
fn inline_puts_the_deleted_lines_of_a_hunk_above_its_added_ones() {
    let (_, side) = inline("a\nb\nc", "a\nx\ny\nc", false);
    assert_eq!(side.text, "a\nb\nx\ny\nc");
    assert_eq!(side.line_starts, vec![0, 2, 4, 6, 8]);
    assert_eq!(
        numbers(&side),
        vec![Some(1), None, Some(2), Some(3), Some(4)]
    );
    use Status::*;
    assert_eq!(
        statuses(&side),
        vec![Unchanged, Deleted, Added, Added, Unchanged]
    );
    let hunks: Vec<_> = side.lines.iter().map(|line| line.hunk).collect();
    assert_eq!(hunks, vec![None, Some(0), Some(0), Some(0), None]);
    assert!(side.lines[1].old && !side.lines[2].old);
    assert_eq!(side.kinds, vec![Kind::Modified]);
    assert!(side.blocks.is_empty() && side.markers.is_empty());
    assert_eq!(&*side.above, &[0, 0, 0, 0, 0, 0]);
}

#[test]
fn the_shorter_side_of_a_hunk_gets_empty_rows_and_deleted_lines_no_number() {
    let (_, old) = beside("a\nb\nc", "a\nx\ny\nc", Side::Old);
    assert_eq!(old.text, "a\nb\nc");
    assert_eq!(numbers(&old), vec![Some(1), None, Some(3)]);
    assert_eq!(
        old.blocks,
        vec![Block {
            line: 2,
            rows: 1,
            kind: BlockKind::Spacer
        }]
    );
    assert_eq!(&*old.above, &[0, 0, 1, 0]);
    assert!(old.markers.is_empty());

    let (_, new) = beside("a\nb\nc", "a\nx\ny\nc", Side::New);
    assert_eq!(new.text, "a\nx\ny\nc");
    assert_eq!(numbers(&new), vec![Some(1), Some(2), Some(3), Some(4)]);
    assert!(new.blocks.is_empty() && new.markers.is_empty());
}

#[test]
fn the_working_tree_side_marks_where_deleted_lines_were() {
    let (_, new) = beside("a\nb\nc", "a\nc", Side::New);
    assert_eq!(new.text, "a\nc");
    assert_eq!(new.kinds, vec![Kind::Deleted]);
    assert_eq!(
        new.blocks,
        vec![Block {
            line: 1,
            rows: 1,
            kind: BlockKind::Spacer
        }]
    );
    assert_eq!(new.markers, vec![(1, 0)]);
    // The last commit's side shows the lines: no marker there, nor inline.
    let (_, old) = beside("a\nb\nc", "a\nc", Side::Old);
    assert!(old.markers.is_empty());
    let (_, inline) = inline("a\nb\nc", "a\nc", false);
    assert_eq!(inline.text, "a\nb\nc");
    assert!(inline.markers.is_empty());
}

#[test]
fn a_gap_is_one_empty_row_above_the_next_part() {
    let text = "l0\nl1\nl2\nl3\nl4\nl5\nl6";
    let diff = diff(Some(text), text, &[]);
    let rows = [
        Row::Line {
            row: 0,
            status: Status::Unchanged,
        },
        Row::Gap { hidden: 5 },
        Row::Line {
            row: 6,
            status: Status::Unchanged,
        },
    ];
    let side = side_rows(&diff, &rows, true);
    assert_eq!(side.text, "l0\nl6");
    assert_eq!(numbers(&side), vec![Some(1), Some(7)]);
    assert_eq!(
        side.blocks,
        vec![Block {
            line: 1,
            rows: 1,
            kind: BlockKind::Gap
        }]
    );
    assert_eq!(&*side.above, &[0, 1, 0]);
}

#[test]
fn changed_words_land_on_the_lines_that_show_them() {
    let (_, side) = inline("let a = 1;\n", "let b = 1;\n", false);
    assert_eq!(side.text, "let a = 1;\nlet b = 1;\n");
    assert_eq!(
        side.words,
        vec![(4..5, Status::Deleted), (15..16, Status::Added)]
    );
    let (_, old) = beside("let a = 1;\n", "let b = 1;\n", Side::Old);
    assert_eq!(old.words, vec![(4..5, Status::Deleted)]);
    let (_, new) = beside("let a = 1;\n", "let b = 1;\n", Side::New);
    assert_eq!(new.words, vec![(4..5, Status::Added)]);
}
