//! The cursor and the selection of the diff view: the line under the cursor, the step to the
//! next change (Alt+F5), the lines picked for Ask agent, and the focus.

use gpui::EntityId;
use ui::prelude::*;

use super::change_rules::{Side, Step, change_anchor, change_at, step_change, whole_lines};
use super::comment_rules::Quote;
use super::diff_rows::{SideRows, line_ranges};
use super::diff_view::DiffView;
use super::review_rules::inline_old_spot;
use crate::files::file_editor_body::line_offset;
use crate::text_view::diff_side::{DiffSide, Reveal};

/// The offset of `column` (bytes, clamped to the line) of row `row` of a file of the side: the
/// last commit's (`old`) or the working tree's. None when the row does not show.
fn offset_of(rows: &SideRows, old: bool, row: u32, column: usize) -> Option<usize> {
    let index = rows.line_of(old, row)?;
    let start = *rows.line_starts.get(index)?;
    Some(start + column.min(line_end(rows, index) - start))
}

/// The offset where line `index` of the side's text ends, its line break left out.
fn line_end(rows: &SideRows, index: usize) -> usize {
    rows.line_starts
        .get(index + 1)
        .map_or(rows.text_len(), |next| next - 1)
}

impl DiffView {
    /// The line of the working tree under the cursor, 1-based. On a deleted line of an inline
    /// diff it is that line's row in the last commit, as Zed's editor gave it.
    pub fn cursor_line(&self, cx: &mut App) -> Option<u32> {
        let right = &self.sides.as_ref()?.right;
        let head = right.selection(cx).1;
        right
            .rows(cx, |rows| {
                let line = rows.lines.get(rows.line_at(head))?;
                Some(line.row + 1)
            })
            .flatten()
    }

    /// Alt+F5 and Shift+Alt+F5: the cursor goes to the next or the previous change, which
    /// scrolls to the top. Changes wrap at both ends.
    pub fn step_change(&mut self, by: Step, window: &mut Window, cx: &mut Context<Self>) {
        let changes = self.changes();
        let line = self.cursor_line(cx);
        let Some(index) = step_change(&changes, line, by) else {
            return;
        };
        let Some(sides) = &self.sides else {
            return;
        };
        let row = change_anchor(&changes[index]).0.saturating_sub(1);
        let right = sides.right.clone();
        if let Some(at) = right
            .rows(cx, |rows| offset_of(rows, false, row, 0))
            .flatten()
        {
            right.select(at..at, Reveal::Top, window, cx);
        }
    }

    /// The change the cursor is in, for "2 of 7".
    pub fn change_index(&self, cx: &mut App) -> Option<usize> {
        let line = self.cursor_line(cx)?;
        change_at(&self.changes(), line)
    }

    /// The whole lines picked in either side, for Ask agent. None without a selection.
    pub fn selection(&self, window: &Window, cx: &mut App) -> Option<Quote> {
        let sides = self.sides.as_ref()?;
        let diff = self.diff.as_ref()?;
        let left_focused = sides
            .left
            .as_ref()
            .is_some_and(|left| left.focus_handle(cx).contains_focused(window, cx));
        let (side, which) = match (&sides.left, left_focused) {
            (Some(left), true) => (left, Side::Old),
            _ => (&sides.right, Side::New),
        };
        let (anchor, head) = side.selection(cx);
        if anchor == head {
            return None;
        }
        let (start, end) = (anchor.min(head), anchor.max(head));
        side.rows(cx, |rows| {
            let (first_index, last_index) = (rows.line_at(start), rows.line_at(end));
            let (first, last) = (rows.lines.get(first_index)?, rows.lines.get(last_index)?);
            let column = end - rows.line_starts.get(last_index)?;
            let (from, to) = whole_lines(first.row + 1, last.row + 1, column as u32 + 1);
            // The text of the file the selection starts in, as Zed's editor took it.
            let text = if first.old { &diff.old } else { &diff.new };
            let lines = line_ranges(text);
            let (Some(top), Some(bottom)) = (
                lines.get(from.saturating_sub(1) as usize),
                lines.get(to.saturating_sub(1) as usize),
            ) else {
                return None;
            };
            Some(Quote {
                side: which,
                start: from,
                end: to,
                text: text.get(top.start..bottom.end)?.to_owned(),
            })
        })
        .flatten()
    }

    /// Picks whole lines `first` to `last` (1-based) of the working tree, as a drag does.
    pub fn select_lines(
        &mut self,
        first: u32,
        last: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(sides) = &self.sides else {
            return;
        };
        let right = sides.right.clone();
        let (first, last) = (first.saturating_sub(1), last.saturating_sub(1));
        let range = right
            .rows(cx, |rows| {
                let start = offset_of(rows, false, first, 0)?;
                let index = rows.line_of(false, last)?;
                Some(start..line_end(rows, index))
            })
            .flatten();
        if let Some(range) = range {
            right.select(range, Reveal::No, window, cx);
        }
    }

    /// The cursor goes to `line` (1-based) of a side and the view scrolls to it. An old line of
    /// an inline diff goes to the place its block has in the working tree.
    pub fn go_to_line(
        &mut self,
        side: Side,
        line: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(sides), Some(diff)) = (&self.sides, &self.diff) else {
            return;
        };
        let row = line.saturating_sub(1);
        let (target, old, row) = match (side, &sides.left) {
            (Side::Old, Some(left)) => (left.clone(), true, row),
            (Side::Old, None) => (
                sides.right.clone(),
                false,
                inline_old_spot(row, &self.hunks).row(),
            ),
            _ => (sides.right.clone(), false, row),
        };
        // A row past the end goes to the last one, as an anchor of Zed's editor clips.
        let rows = if old {
            diff.old_rows()
        } else {
            diff.new_rows()
        };
        move_to(&target, old, row.min(rows.saturating_sub(1)), 0, window, cx);
    }

    /// Go to line (Ctrl+G): `line` and `column` (1-based) of the working tree, clamped to its
    /// text; the working tree's side takes the focus.
    pub fn go_to_new_line(
        &mut self,
        line: u32,
        column: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(sides), Some(diff)) = (&self.sides, &self.diff) else {
            return;
        };
        let at = line_offset(&diff.new, line, column);
        let row = diff.new[..at].matches('\n').count() as u32;
        let line_start = diff.new[..at].rfind('\n').map_or(0, |found| found + 1);
        let right = sides.right.clone();
        move_to(&right, false, row, at - line_start, window, cx);
        right.focus(window, cx);
    }

    /// The side that has the focus, for find; the working tree when neither has it.
    pub fn focused_side(&self, window: &Window, cx: &App) -> Option<DiffSide> {
        let sides = self.sides.as_ref()?;
        let left = sides
            .left
            .as_ref()
            .filter(|left| left.focus_handle(cx).contains_focused(window, cx));
        Some(left.unwrap_or(&sides.right).clone())
    }

    /// `side` is one of the shown sides.
    pub fn shows_side(&self, side: EntityId) -> bool {
        self.sides.as_ref().is_some_and(|sides| {
            sides.right.id() == side || sides.left.as_ref().map(DiffSide::id) == Some(side)
        })
    }

    /// The side that takes the keyboard: the working tree.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(sides) = &self.sides {
            sides.right.focus(window, cx);
        }
    }

    /// The items of Go to symbol: those of the working tree's file.
    pub fn outline_rows(&self, cx: &App) -> Option<Vec<crate::files::outline_rules::OutlineRow>> {
        Some(self.sides.as_ref()?.right.outline_rows(cx))
    }
}

/// The cursor goes to `column` of row `row` of a file of `side`, centred.
fn move_to(side: &DiffSide, old: bool, row: u32, column: usize, window: &mut Window, cx: &mut App) {
    if let Some(at) = side
        .rows(cx, |rows| offset_of(rows, old, row, column))
        .flatten()
    {
        side.select(at..at, Reveal::Center, window, cx);
    }
}
