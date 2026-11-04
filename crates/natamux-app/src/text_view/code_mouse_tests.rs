//! T-MOUSE-1, gutter cases (plan 7.4, S4.1): presses on the line numbers of the editor tab. Each
//! case records the selections after each mouse step; the own code view must give the same.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use editor::display_map::DisplayRow;
use editor::{DisplayPoint, Editor};
use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point,
};
use serde_json::{Value, json};
use ui::prelude::*;

use super::code_typing_tests::{Host, open, start, state};
use crate::app::golden_files;

const SET: &str = "code_typing";
const NAME: &str = "gutter-mouse";
const TEXT: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    println!(\"{a}{b}\");\n}\n";
/// Inside the gutter, left of the line numbers.
const GUTTER_X: f32 = 12.;

/// The middle of display row `row` in the gutter, in window pixels.
fn gutter(editor: &Entity<Editor>, row: u32, cx: &mut VisualTestContext) -> Point<Pixels> {
    editor.update_in(cx, |editor, window, cx| {
        let bounds = *editor.last_bounds().expect("the editor was drawn");
        let snapshot = editor.snapshot(window, cx);
        let mut y = |row: u32| {
            let at = DisplayPoint::new(DisplayRow(row), 0);
            editor
                .display_to_pixel_point(at, &snapshot, window, cx)
                .expect("the row is shown")
                .y
        };
        let (top, next) = (y(row), y(row + 1));
        point(
            bounds.origin.x + px(GUTTER_X),
            bounds.origin.y + top + (next - top) / 2.,
        )
    })
}

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

fn record(
    rows: &mut Vec<Value>,
    case: &str,
    step: usize,
    editor: &Entity<Editor>,
    cx: &mut VisualTestContext,
) {
    cx.run_until_parked();
    let (_, selections) = state(editor, cx);
    rows.push(json!([case, step, selections]));
}

fn run(cx: &mut TestAppContext) -> Vec<Value> {
    let host = Host::new("gutter", "x.rs");
    let (slot, cx) = start(cx);
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let mut rows = Vec::new();

    let editor = open(&host, &format!("ˇ{TEXT}"), &slot, cx);
    let at = gutter(&editor, 2, cx);
    down(at, none, 1, cx);
    record(&mut rows, "click", 1, &editor, cx);
    up(at, none, 1, cx);
    record(&mut rows, "click", 2, &editor, cx);

    let editor = open(&host, &format!("ˇ{TEXT}"), &slot, cx);
    let (from, to) = (gutter(&editor, 1, cx), gutter(&editor, 3, cx));
    down(from, none, 1, cx);
    record(&mut rows, "drag down", 1, &editor, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    record(&mut rows, "drag down", 2, &editor, cx);
    up(to, none, 1, cx);
    record(&mut rows, "drag down", 3, &editor, cx);

    let editor = open(&host, &format!("ˇ{TEXT}"), &slot, cx);
    let (from, to) = (gutter(&editor, 3, cx), gutter(&editor, 1, cx));
    down(from, none, 1, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    up(to, none, 1, cx);
    record(&mut rows, "drag up", 1, &editor, cx);

    let editor = open(&host, &TEXT.replacen("main", "maˇin", 1), &slot, cx);
    let at = gutter(&editor, 3, cx);
    down(at, shift, 1, cx);
    up(at, shift, 1, cx);
    record(&mut rows, "shift click", 1, &editor, cx);

    for clicks in [2, 3] {
        let case = format!("{clicks} clicks");
        let editor = open(&host, &format!("ˇ{TEXT}"), &slot, cx);
        let at = gutter(&editor, 2, cx);
        for count in 1..=clicks {
            down(at, none, count, cx);
            up(at, none, count, cx);
            record(&mut rows, &case, count, &editor, cx);
        }
    }
    rows
}

#[gpui::test]
fn gutter_presses_match_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, NAME, &run(cx));
}

#[gpui::test]
#[ignore]
fn golden_gutter_mouse(cx: &mut TestAppContext) {
    golden_files::write(SET, NAME, &run(cx));
}
