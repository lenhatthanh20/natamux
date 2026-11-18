//! T-MOUSE-1, diff cases (plan 6.5, 7.4), on the own diff view: the same presses as the record
//! made on Zed's editor before S7 must give the same selections, in the rows and bytes
//! of each side's text: a click, 2 and 3 clicks, a drag, a shift click, presses along the gutter
//! (line numbers, the marker strip, the rest), and a press inside a selection with and without a
//! move, before and after 300 ms.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::Duration;

use gpui::{
    Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point,
};
use serde_json::{Value, json};
use ui::prelude::*;

use super::{DiffOptions, DiffView};
use crate::app::{golden_files, test_views};
use crate::text_view::diff_side::{DiffSide, Reveal};

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

/// The left edge of the side and the middle of shown row `row`, in window pixels, and the x of
/// the middle of char `column` from the start of the text (the start itself for None).
fn place(
    side: &DiffSide,
    row: u32,
    column: Option<u32>,
    cx: &mut VisualTestContext,
) -> (Pixels, Pixels, Pixels) {
    cx.update(|window, cx| {
        let mut at = |column: u32| {
            side.row_point(row as usize, column as usize, window, cx)
                .expect("the side was drawn")
        };
        let (left, y, from) = at(column.unwrap_or(0));
        let x = match column {
            Some(column) => from + (at(column + 1).2 - from) / 2.,
            None => px(0.),
        };
        (left, y, x)
    })
}

/// The window pixels of shown row `row` at char `column` of the target's text.
fn at(target: &Target, row: u32, column: u32, cx: &mut VisualTestContext) -> Point<Pixels> {
    let (left, y, x) = place(&target.side, row, Some(column), cx);
    point(left + target.text_x + x, y)
}

/// Where the text starts, from the left edge of the side: the first x, in steps of 1 px, whose
/// press on the first row puts a caret there instead of selecting the line or doing nothing.
fn find_text_x(side: &DiffSide, cx: &mut VisualTestContext) -> Pixels {
    let (left, y, _) = place(side, 0, None, cx);
    let none = Modifiers::default();
    for x in 0..400 {
        select(side, (4, 0), (4, 0), cx);
        let press = point(left + px(x as f32), y);
        down(press, none, 1, cx);
        up(press, none, 1, cx);
        let outcome = selections(side, cx);
        if outcome.as_array().is_some_and(|all| {
            all.len() == 1 && all[0][0] == all[0][2] && all[0][1] == all[0][3] && all[0][0] == 0
        }) {
            return px(x as f32);
        }
    }
    panic!("no x of the first row puts a caret in the text");
}

/// The row and the byte column of offset `at` of the side's text.
fn row_column(side: &DiffSide, at: usize, cx: &mut VisualTestContext) -> (usize, usize) {
    cx.update(|_, cx| {
        side.rows(cx, |rows| {
            let row = rows.line_at(at);
            (row, at - rows.line_starts[row])
        })
        .expect("a diff side")
    })
}

/// The selection as `[tail row, tail column, head row, head column]`, in rows of the side's text.
fn selections(side: &DiffSide, cx: &mut VisualTestContext) -> Value {
    cx.run_until_parked();
    let (anchor, head) = cx.update(|_, cx| side.selection(cx));
    let (tail, head) = (row_column(side, anchor, cx), row_column(side, head, cx));
    json!([[tail.0, tail.1, head.0, head.1]])
}

fn select(side: &DiffSide, from: (u32, u32), to: (u32, u32), cx: &mut VisualTestContext) {
    let offset = |(row, column): (u32, u32), cx: &mut VisualTestContext| {
        cx.update(|_, cx| {
            side.rows(cx, |rows| rows.line_starts[row as usize] + column as usize)
                .expect("a diff side")
        })
    };
    let (from, to) = (offset(from, cx), offset(to, cx));
    cx.update(|window, cx| side.select(from..to, Reveal::No, window, cx));
    cx.run_until_parked();
}

/// The side, and a shown row with text (the changed line `three FOUR`, or `three four` on the
/// old side) whose next row has text too.
struct Target {
    name: &'static str,
    side: DiffSide,
    row: u32,
    /// Where the text starts, from the left edge of the side.
    text_x: Pixels,
}

/// Shows the texts side by side or inline and returns the sides. The rows: the old side has
/// `three four` at 1; the new side `three FOUR` at 1; the inline side shows the deleted line at 1
/// and `three FOUR` at 2.
fn show(
    view: &gpui::Entity<DiffView>,
    side_by_side: bool,
    cx: &mut VisualTestContext,
) -> Vec<Target> {
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
                ignore_whitespace: false,
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
    let target = |name, side: DiffSide, row, cx: &mut VisualTestContext| Target {
        name,
        text_x: find_text_x(&side, cx),
        side,
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
        selections(&target.side, cx)
    ]));
}

/// The outcome of a press at each x from the left edge of the side to the text, in steps of
/// 2 px: runs of equal outcomes as `[from, to, selections after the press]`. The caret starts at
/// the last row, so a selected line shows.
fn probe(target: &Target, row: u32, cx: &mut VisualTestContext) -> Value {
    let side = &target.side;
    let (left, y, _) = place(side, row, None, cx);
    let none = Modifiers::default();
    let mut runs: Vec<(f32, f32, Value)> = Vec::new();
    let mut x = 0.;
    while px(x) < target.text_x {
        select(side, (4, 0), (4, 0), cx);
        let press = point(left + px(x), y);
        down(press, none, 1, cx);
        up(press, none, 1, cx);
        let outcome = selections(side, cx);
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
    let (side, row) = (&target.side, target.row);

    select(side, (4, 0), (4, 0), cx);
    let place = at(target, row, 2, cx);
    down(place, none, 1, cx);
    record(rows, "click", 1, target, cx);
    up(place, none, 1, cx);
    record(rows, "click", 2, target, cx);

    for clicks in [2, 3] {
        select(side, (4, 0), (4, 0), cx);
        let place = at(target, row, 8, cx);
        for count in 1..=clicks {
            down(place, none, count, cx);
            up(place, none, count, cx);
            record(rows, &format!("{clicks} clicks"), count, target, cx);
        }
    }

    select(side, (4, 0), (4, 0), cx);
    let (from, to) = (at(target, row, 1, cx), at(target, row + 1, 3, cx));
    down(from, none, 1, cx);
    record(rows, "drag", 1, target, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    record(rows, "drag", 2, target, cx);
    up(to, none, 1, cx);
    record(rows, "drag", 3, target, cx);

    select(side, (0, 0), (0, 0), cx);
    let place = at(target, row, 3, cx);
    down(place, shift, 1, cx);
    up(place, shift, 1, cx);
    record(rows, "shift click", 1, target, cx);

    // Presses inside a selection (`three` of the row, columns 0 to 5).
    let inside = at(target, row, 2, cx);
    let outside = at(target, row + 1, 4, cx);
    for (case, modifiers, wait, moved) in [
        ("press and release", none, false, false),
        ("move before 300 ms", none, false, true),
        ("move after 300 ms", none, true, true),
        ("move after 300 ms, copy keys", copy, true, true),
    ] {
        select(side, (row, 0), (row, 5), cx);
        down(inside, modifiers, 1, cx);
        record(rows, case, 1, target, cx);
        if wait {
            // The own box reads the time from the test clock.
            cx.executor().advance_clock(Duration::from_millis(350));
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
