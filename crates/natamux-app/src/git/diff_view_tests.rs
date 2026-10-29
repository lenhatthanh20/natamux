//! Both sides of a side by side diff have the same rows, so either side scrolls to its end
//! (T-0230).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::Editor;
use editor::display_map::ToDisplayPoint;
use gpui::{Entity, TestAppContext, VisualTestContext};
use language::Buffer;
use text::Point;
use ui::prelude::*;

use super::{DiffOptions, DiffView};
use crate::app::test_views;

fn numbered(word: &str, count: u32) -> String {
    (0..count).map(|line| format!("{word} {line}\n")).collect()
}

/// The text without its final line break, when `final_break` is false.
fn ended(text: String, final_break: bool) -> String {
    if final_break {
        text
    } else {
        text.trim_end_matches('\n').to_owned()
    }
}

/// The shape of the bug: 3 lines changed, then 147 lines deleted at the end of the file.
fn changed_then_deleted(final_break: bool) -> (String, String) {
    let original = numbered("kept", 277) + &numbered("old", 3) + &numbered("gone", 147);
    let modified = numbered("kept", 277) + &numbered("new", 3);
    (ended(original, final_break), ended(modified, final_break))
}

fn deleted_at_end(final_break: bool) -> (String, String) {
    let original = numbered("kept", 280) + &numbered("gone", 147);
    let modified = numbered("kept", 280);
    (ended(original, final_break), ended(modified, final_break))
}

fn swapped((original, modified): (String, String)) -> (String, String) {
    (modified, original)
}

/// All end-of-file shapes, with a name for the failure message.
fn cases() -> Vec<(String, (String, String))> {
    let mut cases = Vec::new();
    for final_break in [true, false] {
        let shapes = [
            ("changed then deleted", changed_then_deleted(final_break)),
            ("deleted at end", deleted_at_end(final_break)),
            (
                "changed then added",
                swapped(changed_then_deleted(final_break)),
            ),
            ("added at end", swapped(deleted_at_end(final_break))),
        ];
        for (name, texts) in shapes {
            cases.push((format!("{name}, final break {final_break}"), texts));
        }
    }
    cases
}

fn open(cx: &mut TestAppContext) -> (Entity<DiffView>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.add_window_view(|_, _| DiffView::new())
}

fn show(
    view: &Entity<DiffView>,
    key: &str,
    (original, modified): &(String, String),
    collapse: bool,
    cx: &mut VisualTestContext,
) {
    let options = DiffOptions {
        side_by_side: true,
        collapse,
        wrap: false,
    };
    view.update_in(cx, |view, window, cx| {
        view.show(
            key,
            "notes.txt",
            1,
            Some(original),
            modified,
            options,
            window,
            cx,
        )
    });
    cx.run_until_parked();
}

fn last_row(editor: &Entity<Editor>, cx: &mut App) -> u32 {
    editor.update(cx, |editor, cx| editor.max_point(cx).row().0)
}

/// The display row of `row` of `buffer` in `editor`; None when no excerpt holds it.
fn display_row(
    editor: &Entity<Editor>,
    buffer: &Entity<Buffer>,
    row: u32,
    cx: &mut App,
) -> Option<u32> {
    editor.update(cx, |editor, cx| {
        let anchor =
            editor
                .buffer()
                .read(cx)
                .buffer_point_to_anchor(buffer, Point::new(row, 0), cx)?;
        let snapshot = editor.display_snapshot(cx);
        Some(anchor.to_display_point(&snapshot).row().0)
    })
}

/// What the failure message needs: the last rows of both sides, the hunks and the spacers.
struct Shown {
    left: u32,
    right: u32,
    /// The display rows of the first row after each hunk, left and right, where both sides
    /// have that row.
    after_hunks: Vec<(u32, u32)>,
    report: String,
}

fn shown(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> Shown {
    view.update(cx, |view, cx| {
        let sides = view.sides.as_ref().expect("the diff is drawn");
        let left = sides.left.clone().expect("side by side");
        let right = sides.right.clone();
        let buffer = view.buffer.clone().expect("a buffer");
        let base = view
            .diff
            .as_ref()
            .expect("a diff")
            .read(cx)
            .base_text_buffer()
            .clone();
        let last_old = base.read(cx).max_point().row;
        let last_new = buffer.read(cx).max_point().row;
        let mut after_hunks = Vec::new();
        for hunk in &view.hunks {
            if hunk.old.end > last_old || hunk.new.end > last_new {
                continue;
            }
            let old = display_row(&left, &base, hunk.old.end, cx);
            let new = display_row(&right, &buffer, hunk.new.end, cx);
            if let (Some(old), Some(new)) = (old, new) {
                after_hunks.push((old, new));
            }
        }
        let report = format!(
            "hunks {:?}, spacers {:?}, blocks placed {}, last rows old {last_old} new {last_new}",
            view.hunks,
            super::spacers(&view.hunks),
            sides.spacer_blocks.len(),
        );
        Shown {
            left: last_row(&left, cx),
            right: last_row(&right, cx),
            after_hunks,
            report,
        }
    })
}

fn check_all(collapse: bool, cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (name, texts) in cases() {
        show(&view, &name, &texts, collapse, cx);
        let shown = shown(&view, cx);
        assert_eq!(
            shown.left, shown.right,
            "{name}, collapse {collapse}: the sides end on different rows; {}",
            shown.report
        );
        for (old, new) in &shown.after_hunks {
            assert_eq!(
                old, new,
                "{name}, collapse {collapse}: the rows after a hunk do not line up; {}",
                shown.report
            );
        }
    }
}

#[gpui::test]
fn both_sides_end_on_the_same_row_with_the_whole_file(cx: &mut TestAppContext) {
    check_all(false, cx);
}

#[gpui::test]
fn both_sides_end_on_the_same_row_with_unchanged_lines_collapsed(cx: &mut TestAppContext) {
    check_all(true, cx);
}

/// A newer read of the shown file adds lines at its end: the empty rows follow.
#[gpui::test]
fn a_newer_read_with_lines_added_at_the_end_keeps_the_sides_level(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    show(&view, "file", &changed_then_deleted(true), false, cx);
    let (original, _) = changed_then_deleted(true);
    let longer = original.clone() + &numbered("more", 40);
    let options = DiffOptions {
        side_by_side: true,
        collapse: false,
        wrap: false,
    };
    view.update_in(cx, |view, window, cx| {
        view.show(
            "file",
            "notes.txt",
            2,
            Some(&original),
            &longer,
            options,
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let shown = shown(&view, cx);
    assert_eq!(shown.left, shown.right, "{}", shown.report);
}

/// Scrolling the longer side to its end leaves both sides there: the other side follows and
/// does not pull it back. Regression (T-0230): drawn empty rows shrank to one row, so the shorter
/// side ended early and pulled the other back.
#[gpui::test]
fn scrolling_one_side_to_the_end_is_not_pulled_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (name, texts) in [
        ("deleted", changed_then_deleted(true)),
        ("added", swapped(changed_then_deleted(true))),
    ] {
        show(&view, name, &texts, false, cx);
        let (left, right) = view.read_with(cx, |view, _| {
            let sides = view.sides.as_ref().expect("the diff is drawn");
            (
                sides.left.clone().expect("side by side"),
                sides.right.clone(),
            )
        });
        let end = shown(&view, cx).left as f64;
        for scrolled in [&left, &right] {
            // Pass the empty rows on the way, as a wheel does, so they are drawn and measured.
            for top in [270., end] {
                scrolled.update_in(cx, |editor, window, cx| {
                    editor.set_scroll_position(gpui::point(0., top), window, cx);
                });
                cx.run_until_parked();
            }
            for editor in [&left, &right] {
                let top = editor.update(cx, |editor, cx| editor.scroll_position(cx).y);
                assert_eq!(top, end, "{name}: a side did not stay at the end");
            }
            scrolled.update_in(cx, |editor, window, cx| {
                editor.set_scroll_position(gpui::point(0., 0.), window, cx);
            });
            cx.run_until_parked();
        }
    }
}
