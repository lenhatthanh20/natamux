//! Both sides of a side by side diff have the same rows, so either side scrolls to its end
//! (T-0230). The revert arrows ask for the right hunk, and Mod+Z reaches their undo (T-0314).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::RefCell;
use std::rc::Rc;

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, TestAppContext,
    VisualTestContext, point,
};
use ui::prelude::*;

use super::{DiffEvent, DiffOptions, DiffView, RIGHT};
use crate::app::shortcuts::UndoMove;
use crate::app::test_views;
use crate::git::change_rules::Side;
use crate::git::diff_cases::{
    cases, changed_then_deleted, ended, numbered, several_hunks, swapped,
};
use crate::git::diff_model::{Row, Status};
use crate::git::diff_rows::BlockKind;
use crate::git::diff_rules::revert_hunk;
use crate::text_view::diff_side::DiffSide;

fn open(cx: &mut TestAppContext) -> (Entity<DiffView>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.add_window_view(|_, _| DiffView::new())
}

fn options(collapse: bool) -> DiffOptions {
    DiffOptions {
        side_by_side: true,
        collapse,
        wrap: false,
        ignore_whitespace: false,
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

fn sides(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> (DiffSide, DiffSide) {
    view.read_with(cx, |view, _| {
        let sides = view.sides.as_ref().expect("the diff is drawn");
        (
            sides.left.clone().expect("side by side"),
            sides.right.clone(),
        )
    })
}

/// The last shown row of a side, empty rows included.
fn last_row(side: &DiffSide, cx: &mut VisualTestContext) -> usize {
    cx.update(|window, cx| side.place(window, cx).expect("the side was drawn").rows() - 1)
}

/// The shown row of row `row` of the side's file (`old`: the last commit's); None when it does
/// not show.
fn shown_row(side: &DiffSide, old: bool, row: u32, cx: &mut VisualTestContext) -> Option<usize> {
    let line = cx.update(|_, cx| side.rows(cx, |rows| rows.line_of(old, row)).flatten())?;
    cx.update(|window, cx| side.place(window, cx)?.first_row(line))
}

/// What the failure message needs: the last rows of both sides, the hunks and the spacers.
struct Shown {
    left: usize,
    right: usize,
    /// The shown rows of the first row after each hunk, left and right, where both sides have
    /// that row.
    after_hunks: Vec<(usize, usize)>,
    spacers: usize,
    report: String,
}

fn shown(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> Shown {
    let (left, right) = sides(view, cx);
    let (hunks, last_old, last_new) = view.read_with(cx, |view, _| {
        let diff = view.diff.as_ref().expect("a diff");
        (view.hunks.clone(), diff.old_rows() - 1, diff.new_rows() - 1)
    });
    let mut after_hunks = Vec::new();
    for hunk in &hunks {
        if hunk.old.end > last_old || hunk.new.end > last_new {
            continue;
        }
        let old = shown_row(&left, true, hunk.old.end, cx);
        let new = shown_row(&right, false, hunk.new.end, cx);
        if let (Some(old), Some(new)) = (old, new) {
            after_hunks.push((old, new));
        }
    }
    let spacers: usize = cx.update(|_, cx| {
        [&left, &right]
            .iter()
            .map(|side| {
                side.rows(cx, |rows| {
                    rows.blocks
                        .iter()
                        .filter(|block| block.kind == BlockKind::Spacer)
                        .count()
                })
                .unwrap_or(0)
            })
            .sum()
    });
    let report = format!(
        "hunks {hunks:?}, spacers {:?}, blocks placed {spacers}, last rows old {last_old} new \
         {last_new}",
        super::super::diff_rules::spacers(&hunks),
    );
    Shown {
        left: last_row(&left, cx),
        right: last_row(&right, cx),
        after_hunks,
        spacers,
        report,
    }
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

/// Empty rows past the last row still take their rows below the end, so they are never dropped.
#[gpui::test]
fn a_spacer_past_the_last_row_goes_below_the_end(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    show(
        &view,
        "file",
        &("a\nb\nc".into(), "a\nb\nc".into()),
        false,
        cx,
    );
    let (_, right) = sides(&view, cx);
    let before = last_row(&right, cx);
    let rows = [
        Row::Line {
            row: 0,
            status: crate::git::diff_model::Status::Unchanged,
        },
        Row::Line {
            row: 1,
            status: crate::git::diff_model::Status::Unchanged,
        },
        Row::Line {
            row: 2,
            status: crate::git::diff_model::Status::Unchanged,
        },
        Row::Spacer { rows: 5 },
    ];
    let diff = view.read_with(cx, |view, _| view.diff.clone().expect("a diff"));
    cx.update(|_, cx| right.show("notes.txt", &diff, &rows, true, cx));
    cx.run_until_parked();
    assert_eq!(last_row(&right, cx), before + 5, "the spacer took no rows");
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
        let (left, right) = sides(&view, cx);
        let end = shown(&view, cx).left as f32;
        let line_height = cx.update(|window, cx| {
            right
                .place(window, cx)
                .expect("the side was drawn")
                .line_height()
        });
        for scrolled in [&left, &right] {
            // Pass the empty rows on the way, as a wheel does, so they are drawn and measured.
            for top in [270., end] {
                cx.update(|_, cx| scrolled.set_scroll(point(px(0.), line_height * top), cx));
                cx.run_until_parked();
            }
            for side in [&left, &right] {
                let top = cx.update(|_, cx| side.scroll(cx).y);
                assert_eq!(
                    top,
                    line_height * end,
                    "{name}: a side did not stay at the end"
                );
            }
            cx.update(|_, cx| scrolled.set_scroll(point(px(0.), px(0.)), cx));
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
        let hunks = view.hunks.clone();
        assert!(hunks.len() >= 3, "{hunks:?}");
        assert_eq!(view.arrow_at(Side::New, hunks[0].new.start), None);
        view.set_movable(true);
        for (index, hunk) in hunks.iter().enumerate() {
            for (side, row) in [(Side::Old, hunk.old.start), (Side::New, hunk.new.start)] {
                let arrow = view.arrow_at(side, row).expect("an arrow");
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

/// T-0314: Mod+Z in a read-only side of the diff undoes the last revert; the box's own undo,
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

/// The buttons of changes drawn over the working tree's side.
fn buttons(view: &Entity<DiffView>, cx: &mut VisualTestContext) -> usize {
    cx.run_until_parked();
    view.update_in(cx, |view, window, cx| {
        let side = view
            .sides
            .as_ref()
            .expect("the diff is drawn")
            .right
            .clone();
        let place = side.place(window, cx).expect("the side was drawn");
        view.hunk_buttons(&side, RIGHT, &place, false, cx).len()
    })
}

/// The caret moving in a side tells the Changes tab once, and puts the buttons on its change.
#[gpui::test]
fn a_caret_move_is_told_once_and_shows_the_buttons_of_its_change(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    let told = Rc::new(RefCell::new(0));
    let seen = told.clone();
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &DiffEvent, _| {
            if *event == DiffEvent::CursorMoved {
                *seen.borrow_mut() += 1;
            }
        })
        .detach();
    });
    show(&view, "file", &several_hunks(), false, cx);
    let hunk = view.read_with(cx, |view, _| view.hunks[1].clone());
    view.update_in(cx, |view, window, cx| {
        view.set_movable(true);
        view.go_to_line(Side::New, hunk.new.start + 1, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(*told.borrow(), 1);
    assert_eq!(
        view.update(cx, |view, cx| view.cursor_line(cx)),
        Some(hunk.new.start + 1)
    );
    assert_eq!(
        buttons(&view, cx),
        1,
        "the caret's change shows its buttons"
    );
    // The side's box tells a change with no move: nothing more to tell.
    let (_, right) = sides(&view, cx);
    right.input().update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
    assert_eq!(*told.borrow(), 1);
}

/// A closed change of an inline diff shows as unchanged lines, as Zed's collapsed hunks: no box
/// and no buttons, only its strip. A closed change of deleted lines only gets its marker, whatever
/// the other changes show, and a press on that marker opens it again.
#[gpui::test]
fn a_closed_change_has_no_box_or_buttons_and_its_marker_opens_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx);
    // Change 0 changes row 1, change 1 deletes the last row.
    let original = "a 0\nold 0\nb 0\nb 1\ngone 0\n";
    let modified = "a 0\nnew 0\nb 0\nb 1\n";
    view.update_in(cx, |view, window, cx| {
        let options = DiffOptions {
            side_by_side: false,
            ..options(false)
        };
        view.show(
            "inline",
            "notes.txt",
            1,
            Some(original),
            modified,
            options,
            window,
            cx,
        );
        view.set_movable(true);
        view.go_to_line(Side::New, 2, window, cx);
    });
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.hunks.len()), 2);
    assert_eq!(buttons(&view, cx), 1, "an open change shows its buttons");
    let rows = |view: &Entity<DiffView>, cx: &mut VisualTestContext| {
        view.read_with(cx, |view, cx| {
            let sides = view.sides.as_ref().expect("the diff is drawn");
            sides.right.rows(cx, Clone::clone).expect("a diff side")
        })
    };
    let marker = |rows: &crate::git::diff_rows::SideRows| {
        rows.markers
            .iter()
            .find(|(_, hunk)| *hunk == 1)
            .map(|(line, _)| *line)
    };

    // Change 1 closes while change 0 still shows its deleted line: change 1 has its marker.
    view.update(cx, |view, cx| view.toggle(1, cx));
    let shown = rows(&view, cx);
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.old && line.hunk == Some(0))
    );
    assert!(
        !shown
            .lines
            .iter()
            .any(|line| line.old && line.hunk == Some(1))
    );
    assert!(
        marker(&shown).is_some(),
        "the closed deletion has its marker"
    );

    // Change 0 closes: its added line keeps the hunk (the strip) but shows as unchanged.
    view.update(cx, |view, cx| view.toggle(0, cx));
    let shown = rows(&view, cx);
    assert_eq!(shown.closed, vec![1, 0]);
    let of_0: Vec<_> = shown
        .lines
        .iter()
        .filter(|line| line.hunk == Some(0))
        .collect();
    assert_eq!(of_0.len(), 1, "{of_0:?}");
    assert!(!of_0[0].old && of_0[0].status == Status::Unchanged);
    assert!(shown.words.is_empty(), "{:?}", shown.words);
    assert_eq!(buttons(&view, cx), 0, "a closed change shows no buttons");
    // Change 0's deleted line went, so the marker sits one line higher.
    let line = marker(&shown).expect("the closed deletion keeps its marker");

    // A press on the marker opens change 1 again; change 0 stays closed.
    let at = view.update_in(cx, |view, window, cx| {
        let side = view
            .sides
            .as_ref()
            .expect("the diff is drawn")
            .right
            .clone();
        let place = side.place(window, cx).expect("the side was drawn");
        let top = match place.line(line) {
            Some((top, _)) => top,
            None => place.block_top(line, 0),
        };
        place.origin + point(px(1.), top)
    });
    let none = Modifiers::default();
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: none,
        click_count: 1,
        first_mouse: false,
    });
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: none,
        click_count: 1,
    });
    cx.run_until_parked();
    assert_eq!(view.read_with(cx, |view, _| view.closed.clone()), vec![0]);
    let shown = rows(&view, cx);
    assert!(
        shown
            .lines
            .iter()
            .any(|line| line.old && line.hunk == Some(1))
    );
    assert_eq!(marker(&shown), None);
}
