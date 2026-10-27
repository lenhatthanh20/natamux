//! The cursor and the selection of the diff view: the line under the cursor, the step to the
//! next change (Alt+F5), the lines picked for Ask agent, and the focus.

use editor::SelectionEffects;
use editor::scroll::Autoscroll;
use gpui::Focusable;
use text::Point;
use ui::prelude::*;

use super::change_rules::{Side, Step, change_anchor, change_at, step_change, whole_lines};
use super::comment_rules::Quote;
use super::diff_view::DiffView;
use super::review_rules::inline_old_spot;

impl DiffView {
    /// The line of the working tree under the cursor, 1-based.
    pub fn cursor_line(&self, cx: &mut App) -> Option<u32> {
        let right = self.sides.as_ref()?.right.clone();
        right.update(cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let head = editor.selections.newest::<Point>(&snapshot).head();
            let multi = editor.buffer().read(cx).snapshot(cx);
            let (_, point) = multi.point_to_buffer_point(head)?;
            Some(point.row + 1)
        })
    }

    /// Alt+F5 and Shift+Alt+F5: the cursor goes to the next or the previous change, which
    /// scrolls to the top. Changes wrap at both ends.
    pub fn step_change(&mut self, by: Step, window: &mut Window, cx: &mut Context<Self>) {
        let changes = self.changes();
        let line = self.cursor_line(cx);
        let Some(index) = step_change(&changes, line, by) else {
            return;
        };
        let (Some(sides), Some(buffer)) = (&self.sides, &self.buffer) else {
            return;
        };
        let row = change_anchor(&changes[index]).0.saturating_sub(1);
        let buffer = buffer.clone();
        sides.right.update(cx, |editor, cx| {
            let anchor =
                editor
                    .buffer()
                    .read(cx)
                    .buffer_point_to_anchor(&buffer, Point::new(row, 0), cx);
            if let Some(anchor) = anchor {
                let effects = SelectionEffects::scroll(Autoscroll::top_relative(0.));
                editor.change_selections(effects, window, cx, |selections| {
                    selections.select_anchor_ranges([anchor..anchor])
                });
            }
        });
    }

    /// The change the cursor is in, for "2 of 7".
    pub fn change_index(&self, cx: &mut App) -> Option<usize> {
        let line = self.cursor_line(cx)?;
        change_at(&self.changes(), line)
    }

    /// The whole lines picked in either side, for Ask agent. None without a selection.
    pub fn selection(&self, window: &Window, cx: &mut App) -> Option<Quote> {
        let sides = self.sides.as_ref()?;
        let left_focused = sides
            .left
            .as_ref()
            .is_some_and(|left| left.read(cx).focus_handle(cx).contains_focused(window, cx));
        let (editor, side) = match (&sides.left, left_focused) {
            (Some(left), true) => (left.clone(), Side::Old),
            _ => (sides.right.clone(), Side::New),
        };
        editor.update(cx, |editor, cx| {
            let snapshot = editor.display_snapshot(cx);
            let picked = editor.selections.newest::<Point>(&snapshot);
            if picked.is_empty() {
                return None;
            }
            let multi = editor.buffer().read(cx).snapshot(cx);
            let (text, start) = multi.point_to_buffer_point(picked.start)?;
            let (_, end) = multi.point_to_buffer_point(picked.end)?;
            let (first, last) = whole_lines(start.row + 1, end.row + 1, end.column + 1);
            let range = Point::new(first - 1, 0)..Point::new(last - 1, text.line_len(last - 1));
            Some(Quote {
                side,
                start: first,
                end: last,
                text: text.text_for_range(range).collect(),
            })
        })
    }

    /// Picks whole lines `first` to `last` (1-based) of the working tree, as a drag does.
    pub fn select_lines(
        &mut self,
        first: u32,
        last: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (Some(sides), Some(buffer)) = (&self.sides, &self.buffer) else {
            return;
        };
        let buffer = buffer.clone();
        let (first, last) = (first.saturating_sub(1), last.saturating_sub(1));
        let end_column = buffer.read(cx).text_snapshot().line_len(last);
        sides.right.update(cx, |editor, cx| {
            let multi = editor.buffer().read(cx);
            let start = multi.buffer_point_to_anchor(&buffer, Point::new(first, 0), cx);
            let end = multi.buffer_point_to_anchor(&buffer, Point::new(last, end_column), cx);
            if let (Some(start), Some(end)) = (start, end) {
                editor.change_selections(SelectionEffects::no_scroll(), window, cx, |selections| {
                    selections.select_anchor_ranges([start..end])
                });
            }
        });
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
        let (Some(sides), Some(buffer), Some(diff)) = (&self.sides, &self.buffer, &self.diff)
        else {
            return;
        };
        let row = line.saturating_sub(1);
        let (editor, buffer, row) = match (side, &sides.left) {
            (Side::Old, Some(left)) => {
                (left.clone(), diff.read(cx).base_text_buffer().clone(), row)
            }
            (Side::Old, None) => {
                let row = inline_old_spot(row, &self.hunks).row();
                (sides.right.clone(), buffer.clone(), row)
            }
            _ => (sides.right.clone(), buffer.clone(), row),
        };
        editor.update(cx, |editor, cx| {
            let anchor =
                editor
                    .buffer()
                    .read(cx)
                    .buffer_point_to_anchor(&buffer, Point::new(row, 0), cx);
            if let Some(anchor) = anchor {
                let effects = SelectionEffects::scroll(Autoscroll::center());
                editor.change_selections(effects, window, cx, |selections| {
                    selections.select_anchor_ranges([anchor..anchor])
                });
            }
        });
    }

    /// The editor that takes the keyboard: the working tree.
    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        if let Some(sides) = &self.sides {
            let handle = sides.right.read(cx).focus_handle(cx);
            window.focus(&handle, cx);
        }
    }
}
