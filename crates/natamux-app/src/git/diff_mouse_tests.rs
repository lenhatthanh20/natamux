//! T-MOUSE-1, diff cases (plan 6.5, 7.4, S6.1): the mouse in the read-only editors of the diff
//! view: a click, 2 and 3 clicks, a drag, a shift click, presses along the gutter (line numbers,
//! the marker strip, the rest), and a press inside a selection with and without a move, before and
//! after 300 ms. Each case records the selections after each step; the own diff view must give
//! the same. The probes of the gutter record which x ranges give which outcome.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use editor::display_map::DisplayRow;
use editor::{DisplayPoint, Editor};
use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point,
};
use serde_json::{Value, json};
use ui::prelude::*;

use super::{DiffOptions, DiffView};
use crate::app::{golden_files, test_views};

const SET: &str = "diff";
const NAME: &str = "mouse";
const OLD: &str = "one two\nthree four\nfive six\nseven eight\nnine ten\n";
const NEW: &str = "one two\nthree FOUR\nfive six\nextra line\nseven eight\nnine ten\n";

fn down(at: Point<Pixels>, modifiers: Modifiers, clicks: usize, cx: &mut VisualTestContext) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: clicks,
        first_mouse: false,
    });
}

fn up(at: Point<Pixels>, modifiers: Modifiers, clicks: usize, cx: &mut VisualTestContext) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: clicks,
    });
}

/// The x of the left edge of the editor and the y of the middle of display row `row`, in window
/// pixels, and the x of the middle of character `column` from the start of the text (the left
/// edge of the text when `column` is None).
fn place(
    editor: &Entity<Editor>,
    row: u32,
    column: Option<u32>,
    cx: &mut VisualTestContext,
) -> (Pixels, Pixels, Pixels) {
    editor.update_in(cx, |editor, window, cx| {
        let bounds = *editor.last_bounds().expect("the editor was drawn");
        let snapshot = editor.snapshot(window, cx);
        let mut pixel = |row: u32, column: u32| {
            editor
                .display_to_pixel_point(
                    DisplayPoint::new(DisplayRow(row), column),
                    &snapshot,
                    window,
                    cx,
                )
                .expect("the row is shown")
        };
        let (top, next) = (pixel(row, 0), pixel(row + 1, 0));
        let (from, to) = match column {
            Some(column) => (pixel(row, column).x, pixel(row, column + 1).x),
            None => (top.x, top.x),
        };
        (
            bounds.origin.x,
            bounds.origin.y + top.y + (next.y - top.y) / 2.,
            from + (to - from) / 2.,
        )
    })
}

/// The window pixels of display row `row` at character `column` of the target's text.
fn at(target: &Target, row: u32, column: u32, cx: &mut VisualTestContext) -> Point<Pixels> {
    let (left, y, x) = place(&target.editor, row, Some(column), cx);
    point(left + target.text_x + x, y)
}

/// Where the text starts, from the left edge of the editor: the first x, in steps of 1 px, whose
/// press on the first row puts a caret there instead of selecting the line or doing nothing.
fn find_text_x(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Pixels {
    let (left, y, _) = place(editor, 0, None, cx);
    let none = Modifiers::default();
    for x in 0..400 {
        select(editor, (4, 0), (4, 0), cx);
        let press = point(left + px(x as f32), y);
        down(press, none, 1, cx);
        up(press, none, 1, cx);
        let outcome = selections(editor, cx);
        if outcome.as_array().is_some_and(|all| {
            all.len() == 1 && all[0][0] == all[0][2] && all[0][1] == all[0][3] && all[0][0] == 0
        }) {
            return px(x as f32);
        }
    }
    panic!("no x of the first row puts a caret in the text");
}

/// Each selection as `[tail row, tail column, head row, head column]`, in buffer rows of the
/// editor's multibuffer.
fn selections(editor: &Entity<Editor>, cx: &mut VisualTestContext) -> Value {
    cx.run_until_parked();
    editor.update(cx, |editor, cx| {
        let snapshot = editor.display_snapshot(cx);
        Value::Array(
            editor
                .selections
                .all_adjusted(&snapshot)
                .iter()
                .map(|selection| {
                    let (tail, head) = (selection.tail(), selection.head());
                    json!([tail.row, tail.column, head.row, head.column])
                })
                .collect(),
        )
    })
}

fn select(editor: &Entity<Editor>, from: (u32, u32), to: (u32, u32), cx: &mut VisualTestContext) {
    editor.update_in(cx, |editor, window, cx| {
        let (from, to) = (
            text::Point::new(from.0, from.1),
            text::Point::new(to.0, to.1),
        );
        editor.change_selections(Default::default(), window, cx, |selections| {
            selections.select_ranges([from..to])
        });
    });
    cx.run_until_parked();
}

/// The editor of one side, and a display row with text (the changed line `three FOUR`, or
/// `three four` on the old side) whose next row has text too.
struct Target {
    name: &'static str,
    editor: Entity<Editor>,
    row: u32,
    /// Where the text starts, from the left edge of the editor.
    text_x: Pixels,
}

/// Shows the texts side by side or inline and returns the editors. The display rows: the old
/// side has `three four` at 1; the new side `three FOUR` at 1; the inline editor shows the
/// deleted line at 1 and `three FOUR` at 2.
fn show(view: &Entity<DiffView>, side_by_side: bool, cx: &mut VisualTestContext) -> Vec<Target> {
    view.update_in(cx, |view, window, cx| {
        view.show(
            if side_by_side { "side" } else { "inline" },
            "notes.txt",
            1,
            Some(OLD),
            NEW,
            DiffOptions {
                side_by_side,
                collapse: false,
                wrap: false,
            },
            window,
            cx,
        )
    });
    cx.run_until_parked();
    let (left, right) = view.read_with(cx, |view, _| {
        let sides = view.sides.as_ref().expect("the diff is drawn");
        (sides.left.clone(), sides.right.clone())
    });
    let target = |name, editor: Entity<Editor>, row, cx: &mut VisualTestContext| Target {
        name,
        text_x: find_text_x(&editor, cx),
        editor,
        row,
    };
    match left {
        Some(left) => vec![target("left", left, 1, cx), target("right", right, 1, cx)],
        None => vec![target("inline", right, 2, cx)],
    }
}

fn record(
    rows: &mut Vec<Value>,
    case: &str,
    step: usize,
    target: &Target,
    cx: &mut VisualTestContext,
) {
    rows.push(json!([
        target.name,
        case,
        step,
        selections(&target.editor, cx)
    ]));
}

/// The outcome of a press at each x from the left edge of the editor to the text, in steps of
/// 2 px: runs of equal outcomes as `[from, to, selections after the press]`. The caret starts at
/// the last row, so a selected line shows.
fn probe(target: &Target, row: u32, cx: &mut VisualTestContext) -> Value {
    let editor = &target.editor;
    let (left, y, _) = place(editor, row, None, cx);
    let none = Modifiers::default();
    let mut runs: Vec<(f32, f32, Value)> = Vec::new();
    let mut x = 0.;
    while px(x) < target.text_x {
        select(editor, (4, 0), (4, 0), cx);
        let press = point(left + px(x), y);
        down(press, none, 1, cx);
        up(press, none, 1, cx);
        let outcome = selections(editor, cx);
        match runs.last_mut() {
            Some((_, to, last)) if *last == outcome => *to = x,
            _ => runs.push((x, x, outcome)),
        }
        x += 2.;
    }
    Value::Array(
        runs.into_iter()
            .map(|(from, to, outcome)| json!([from, to, outcome]))
            .collect(),
    )
}

fn run(cx: &mut TestAppContext) -> Vec<Value> {
    test_views::init(cx);
    let (view, cx) = cx.add_window_view(|_, _| DiffView::new());
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let copy = if cfg!(target_os = "macos") {
        Modifiers {
            alt: true,
            ..Modifiers::default()
        }
    } else {
        Modifiers {
            control: true,
            ..Modifiers::default()
        }
    };
    let mut rows = Vec::new();
    for side_by_side in [true, false] {
        for target in show(&view, side_by_side, cx) {
            cases(&mut rows, &target, (none, shift, copy), cx);
        }
        for target in show(&view, side_by_side, cx) {
            rows.push(json!([target.name, "text x", f32::from(target.text_x)]));
            for row in [0, target.row, target.row + 1] {
                let outcome = probe(&target, row, cx);
                rows.push(json!([target.name, "gutter", row, outcome]));
            }
        }
    }
    rows
}

fn cases(
    rows: &mut Vec<Value>,
    target: &Target,
    (none, shift, copy): (Modifiers, Modifiers, Modifiers),
    cx: &mut VisualTestContext,
) {
    let (editor, row) = (&target.editor, target.row);

    select(editor, (4, 0), (4, 0), cx);
    let place = at(target, row, 2, cx);
    down(place, none, 1, cx);
    record(rows, "click", 1, target, cx);
    up(place, none, 1, cx);
    record(rows, "click", 2, target, cx);

    for clicks in [2, 3] {
        select(editor, (4, 0), (4, 0), cx);
        let place = at(target, row, 8, cx);
        for count in 1..=clicks {
            down(place, none, count, cx);
            up(place, none, count, cx);
            record(rows, &format!("{clicks} clicks"), count, target, cx);
        }
    }

    select(editor, (4, 0), (4, 0), cx);
    let (from, to) = (at(target, row, 1, cx), at(target, row + 1, 3, cx));
    down(from, none, 1, cx);
    record(rows, "drag", 1, target, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    record(rows, "drag", 2, target, cx);
    up(to, none, 1, cx);
    record(rows, "drag", 3, target, cx);

    select(editor, (0, 0), (0, 0), cx);
    let place = at(target, row, 3, cx);
    down(place, shift, 1, cx);
    up(place, shift, 1, cx);
    record(rows, "shift click", 1, target, cx);

    // Presses inside a selection (`three` of the row, columns 0 to 5; the multibuffer's rows are
    // the display rows here, deleted lines of the inline editor included).
    let inside = at(target, row, 2, cx);
    let outside = at(target, row + 1, 4, cx);
    for (case, modifiers, wait, moved) in [
        ("press and release", none, false, false),
        ("move before 300 ms", none, false, true),
        ("move after 300 ms", none, true, true),
        ("move after 300 ms, copy keys", copy, true, true),
    ] {
        select(editor, (row, 0), (row, 5), cx);
        down(inside, modifiers, 1, cx);
        record(rows, case, 1, target, cx);
        if wait {
            std::thread::sleep(Duration::from_millis(350));
        }
        if moved {
            cx.simulate_mouse_move(outside, MouseButton::Left, modifiers);
            record(rows, case, 2, target, cx);
            up(outside, modifiers, 1, cx);
        } else {
            up(inside, modifiers, 1, cx);
        }
        record(rows, case, 3, target, cx);
        let clipboard = cx.read_from_clipboard().and_then(|item| item.text());
        rows.push(json!([target.name, case, "clipboard", clipboard]));
    }
}

#[gpui::test]
fn the_diff_mouse_matches_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, NAME, &run(cx));
}

#[gpui::test]
#[ignore]
fn golden_diff_mouse(cx: &mut TestAppContext) {
    golden_files::write(SET, NAME, &run(cx));
}
