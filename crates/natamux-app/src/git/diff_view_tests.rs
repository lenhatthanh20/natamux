//! Both sides of a side by side diff have the same rows, so either side scrolls to its end
//! (T-0230).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::display_map::ToDisplayPoint;
use editor::{Editor, MultiBuffer};
use gpui::{Entity, TestAppContext, VisualTestContext};
use language::{Buffer, Capability};
use text::Point;
use ui::prelude::*;

use super::{DiffOptions, DiffView};
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_editors::{add_spacer, read_only_editor, set_excerpts};
use crate::git::diff_rules::Spacer;

fn numbered(word: &str, count: u32) -> String {
    (0..count).map(|line| format!("{word} {line}\n")).collect()
}

/// The text without its final line break, when `final_break` is false.
fn ended(text: &str, final_break: bool) -> String {
    if final_break {
        text.to_owned()
    } else {
        text.trim_end_matches('\n').to_owned()
    }
}

/// The shape of the bug: 3 lines changed, then 147 lines deleted at the end of the file.
fn changed_then_deleted() -> (String, String) {
    let original = numbered("kept", 277) + &numbered("old", 3) + &numbered("gone", 147);
    let modified = numbered("kept", 277) + &numbered("new", 3);
    (original, modified)
}

fn deleted_at_end() -> (String, String) {
    let original = numbered("kept", 280) + &numbered("gone", 147);
    let modified = numbered("kept", 280);
    (original, modified)
}

/// Hunks in the middle (changed, added, deleted) and lines deleted at the end.
fn several_hunks() -> (String, String) {
    let original = numbered("a", 50)
        + &numbered("old", 5)
        + &numbered("b", 50)
        + &numbered("c", 50)
        + &numbered("gone", 20)
        + &numbered("d", 30)
        + &numbered("tail", 100);
    let modified = numbered("a", 50)
        + &numbered("new", 2)
        + &numbered("b", 50)
        + &numbered("added", 10)
        + &numbered("c", 50)
        + &numbered("d", 30);
    (original, modified)
}

fn swapped((original, modified): (String, String)) -> (String, String) {
    (modified, original)
}

/// All shapes with a final line break in both texts, in neither, and in only one; with a name
/// for the failure message.
fn cases() -> Vec<(String, (String, String))> {
    let shapes = [
        ("changed then deleted", changed_then_deleted()),
        ("deleted at end", deleted_at_end()),
        ("changed then added", swapped(changed_then_deleted())),
        ("added at end", swapped(deleted_at_end())),
        ("several hunks", several_hunks()),
        ("several hunks added", swapped(several_hunks())),
    ];
    let mut cases = Vec::new();
    for (name, (original, modified)) in shapes {
        for (old_break, new_break) in [(true, true), (false, false), (true, false), (false, true)] {
            cases.push((
                format!("{name}, final breaks {old_break} {new_break}"),
                (ended(&original, old_break), ended(&modified, new_break)),
            ));
        }
    }
    cases
}

fn open(cx: &mut TestAppContext) -> (Entity<DiffView>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.add_window_view(|_, _| DiffView::new())
}

fn options(collapse: bool) -> DiffOptions {
    DiffOptions {
        side_by_side: true,
        collapse,
        wrap: false,
    }
}

fn show_read(
    view: &Entity<DiffView>,
    key: &str,
    version: u64,
    (original, modified): &(String, String),
    collapse: bool,
    cx: &mut VisualTestContext,
) {
    view.update_in(cx, |view, window, cx| {
        view.show(
            key,
            "notes.txt",
            version,
            Some(original),
            modified,
            options(collapse),
            window,
            cx,
        )
    });
    cx.run_until_parked();
}

fn show(
    view: &Entity<DiffView>,
    key: &str,
    texts: &(String, String),
    collapse: bool,
    cx: &mut VisualTestContext,
) {
    show_read(view, key, 1, texts, collapse, cx);
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
    spacers: usize,
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
            spacers: sides.spacer_blocks.len(),
            report,
        }
    })
}

fn assert_level(name: &str, shown: &Shown) {
    assert_eq!(
        shown.left, shown.right,
        "{name}: the sides end on different rows; {}",
        shown.report
    );
    for (old, new) in &shown.after_hunks {
        assert_eq!(
            old, new,
            "{name}: the rows after a hunk do not line up; {}",
            shown.report
        );
    }
}

fn check_all(collapse: bool, cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (name, texts) in cases() {
        show(&view, &name, &texts, collapse, cx);
        assert_level(&format!("{name}, collapse {collapse}"), &shown(&view, cx));
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
    for collapse in [false, true] {
        let key = format!("file {collapse}");
        let (original, modified) = several_hunks();
        show_read(
            &view,
            &key,
            1,
            &(original.clone(), modified.clone()),
            collapse,
            cx,
        );
        let longer = modified + &numbered("more", 40);
        show_read(&view, &key, 2, &(original, longer), collapse, cx);
        assert_level(
            &format!("newer read, collapse {collapse}"),
            &shown(&view, cx),
        );
    }
}

#[gpui::test]
fn identical_texts_get_no_empty_rows(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for collapse in [false, true] {
        for final_break in [true, false] {
            let text = ended(&numbered("kept", 100), final_break);
            let name = format!("identical, final break {final_break}, collapse {collapse}");
            show(&view, &name, &(text.clone(), text), collapse, cx);
            let shown = shown(&view, cx);
            assert_level(&name, &shown);
            assert_eq!(shown.spacers, 0, "{name}: {}", shown.report);
        }
    }
}

/// A spacer whose row no excerpt holds, or past the last row, still takes its rows below the
/// end, so it is never dropped.
#[gpui::test]
fn a_spacer_with_no_anchor_goes_below_the_end(cx: &mut TestAppContext) {
    let (_, cx) = open(cx);
    let (editor, shown_buffer, other) = cx.update(|window, cx| {
        let shown_buffer = cx.new(|cx| Buffer::local("a\nb\nc", cx));
        let other = cx.new(|cx| Buffer::local("x\ny", cx));
        let multi = cx.new(|cx| {
            let mut multi = MultiBuffer::new(Capability::ReadOnly);
            set_excerpts(&mut multi, &shown_buffer, Vec::new(), false, cx);
            multi
        });
        let editor = read_only_editor(multi, &shown_buffer, false, window, cx);
        (editor, shown_buffer, other)
    });
    for (name, buffer, before_row) in [
        ("no excerpt holds the buffer", &other, 1),
        ("past the last row", &shown_buffer, 10),
    ] {
        let before = cx.update(|_, cx| last_row(&editor, cx));
        let spacer = Spacer {
            side: Side::New,
            before_row,
            rows: 5,
        };
        let id = cx.update(|_, cx| add_spacer(&editor, buffer, spacer, cx));
        cx.run_until_parked();
        assert!(id.is_some(), "{name}: the spacer was dropped");
        let after = cx.update(|_, cx| last_row(&editor, cx));
        assert_eq!(after, before + 5, "{name}: the spacer took no rows");
    }
}

/// Scrolling the longer side to its end leaves both sides there: the other side follows and
/// does not pull it back. Regression (T-0230): drawn empty rows shrank to one row, so the shorter
/// side ended early and pulled the other back.
#[gpui::test]
fn scrolling_one_side_to_the_end_is_not_pulled_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (name, texts) in [
        ("deleted", changed_then_deleted()),
        ("added", swapped(changed_then_deleted())),
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
