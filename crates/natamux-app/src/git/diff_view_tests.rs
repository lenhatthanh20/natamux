//! Both sides of a side by side diff have the same rows, so either side scrolls to its end
//! (T-0230). The revert arrows ask for the right hunk, and Mod+Z reaches their undo (T-0314).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use editor::display_map::ToDisplayPoint;
use editor::{Editor, MultiBuffer};
use gpui::{Entity, TestAppContext, VisualTestContext};
use language::{Buffer, Capability};
use text::Point;
use ui::prelude::*;

use super::{DiffEvent, DiffOptions, DiffView};
use crate::app::shortcuts::UndoMove;
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_editors::{add_spacer, read_only_editor, set_excerpts};
use crate::git::diff_rules::{Spacer, revert_hunk};

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

/// T-0314: every hunk's arrow, on either side, asks for the revert of that hunk, and only while
/// the file can be reverted.
#[gpui::test]
fn each_hunk_has_an_arrow_on_both_sides_that_asks_for_its_revert(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let asked = Rc::new(RefCell::new(Vec::new()));
    let seen = asked.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &DiffEvent, _| {
            seen.borrow_mut().push(*event)
        })
        .detach();
    });
    show(&view, "file", &several_hunks(), false, cx);
    view.update(cx, |view, cx| {
        let buffer = view.buffer.clone().expect("a buffer").read(cx).remote_id();
        let diff = view.diff.clone().expect("a diff");
        let base = diff.read(cx).base_text_buffer().read(cx).remote_id();
        let hunks = view.hunks.clone();
        assert!(hunks.len() >= 3, "{hunks:?}");
        assert_eq!(view.arrow_at(buffer, hunks[0].new.start, cx), None);
        view.set_movable(true);
        for (index, hunk) in hunks.iter().enumerate() {
            for (side, row) in [(base, hunk.old.start), (buffer, hunk.new.start)] {
                let arrow = view.arrow_at(side, row, cx).expect("an arrow");
                assert_eq!(arrow.index, index, "{hunk:?}");
                assert!(!arrow.moving);
                view.ask_revert(arrow.index, arrow.version, cx);
            }
        }
        // A read that is not the shown one asks for nothing.
        view.ask_revert(0, view.version + 1, cx);
    });
    cx.run_until_parked();
    let version = view.read_with(cx, |view, _| view.version);
    let count = view.read_with(cx, |view, _| view.hunks.len());
    let expected: Vec<DiffEvent> = (0..count)
        .flat_map(|index| [DiffEvent::RevertHunk { index, version }; 2])
        .collect();
    let asked: Vec<DiffEvent> = asked
        .borrow()
        .iter()
        .filter(|event| matches!(event, DiffEvent::RevertHunk { .. }))
        .copied()
        .collect();
    assert_eq!(asked, expected);
}

/// T-0314: reverting every hunk of the real diff, from the last one up, gives the last
/// commit's text back. A final line break the diff does not count as a change may stay.
#[gpui::test]
fn reverting_every_hunk_gives_the_old_text_back(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    for (name, (original, modified)) in cases() {
        show(
            &view,
            &name,
            &(original.clone(), modified.clone()),
            false,
            cx,
        );
        let hunks = view.read_with(cx, |view, _| view.hunks.clone());
        let reverted = hunks.iter().rev().fold(modified.clone(), |text, hunk| {
            revert_hunk(&original, &text, hunk)
        });
        if original.ends_with('\n') == modified.ends_with('\n') {
            assert_eq!(reverted, original, "{name}: {hunks:?}");
        } else {
            assert_eq!(
                reverted.trim_end_matches('\n'),
                original.trim_end_matches('\n'),
                "{name}: {hunks:?}"
            );
        }
    }
}

/// The Changes tab around the diff, as far as keys go.
struct Host {
    diff: Entity<DiffView>,
    undos: usize,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .key_context("NatamuxChanges")
            .size_full()
            .on_action(cx.listener(|host, _: &UndoMove, _, _| host.undos += 1))
            .child(self.diff.clone())
    }
}

/// T-0314: Mod+Z in the read-only diff editor undoes the last revert; the editor's own undo,
/// bound for every text box, does not take the key.
#[gpui::test]
fn mod_z_in_a_diff_editor_undoes_the_last_revert(cx: &mut TestAppContext) {
    test_views::init(cx);
    cx.update(crate::app::shortcuts::bind);
    let (host, cx) = cx.add_window_view(|_, cx| Host {
        diff: cx.new(|_| DiffView::new()),
        undos: 0,
    });
    let view = host.read_with(cx, |host, _| host.diff.clone());
    show(&view, "file", &several_hunks(), false, cx);
    view.update_in(cx, |view, window, cx| view.focus(window, cx));
    cx.run_until_parked();
    let undo = if cfg!(target_os = "macos") {
        "cmd-z"
    } else {
        "ctrl-z"
    };
    cx.simulate_keystrokes(undo);
    assert_eq!(host.read_with(cx, |host, _| host.undos), 1);
}
