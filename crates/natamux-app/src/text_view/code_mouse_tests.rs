//! T-MOUSE-1, gutter cases (plan 7.4, S4.1): presses on the line numbers of the editor tab. Each
//! case records the selections after each mouse step; the own code view must give the golden
//! record made on Zed's editor before S7.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{
    Entity, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point, TestAppContext,
    VisualTestContext, point,
};
use serde_json::{Value, json};
use ui::prelude::*;

use super::code_typing_tests::{open_own_editor, own_state, start};
use super::text_input::TextInput;
use crate::app::golden_files;

const SET: &str = "code_typing";
const NAME: &str = "gutter-mouse";
const TEXT: &str = "fn main() {\n    let a = 1;\n    let b = 2;\n    println!(\"{a}{b}\");\n}\n";
/// Inside the gutter, left of the line numbers.
const GUTTER_X: f32 = 12.;

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

/// The middle of row `row` in the own view's gutter, in window pixels.
fn own_gutter(input: &Entity<TextInput>, row: u32, cx: &mut VisualTestContext) -> Point<Pixels> {
    input.update_in(cx, |input, window, cx| {
        let bounds = *input.last_bounds().expect("the view was drawn");
        let line_height = input.layout(window, cx).line_height;
        point(
            bounds.origin.x + px(GUTTER_X),
            bounds.origin.y + line_height * (row as f32 + 0.5),
        )
    })
}

fn own_record(
    rows: &mut Vec<Value>,
    case: &str,
    step: usize,
    input: &Entity<TextInput>,
    cx: &mut VisualTestContext,
) {
    cx.run_until_parked();
    let (_, selections) = own_state(input, cx);
    rows.push(json!([case, step, selections]));
}

fn run_own(cx: &mut TestAppContext) -> Vec<Value> {
    let (slot, cx) = start(cx);
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..Modifiers::default()
    };
    let mut rows = Vec::new();
    let open =
        |caret: usize, cx: &mut VisualTestContext| open_own_editor("x.rs", TEXT, caret, &slot, cx);

    let input = open(0, cx);
    let at = own_gutter(&input, 2, cx);
    down(at, none, 1, cx);
    own_record(&mut rows, "click", 1, &input, cx);
    up(at, none, 1, cx);
    own_record(&mut rows, "click", 2, &input, cx);

    let input = open(0, cx);
    let (from, to) = (own_gutter(&input, 1, cx), own_gutter(&input, 3, cx));
    down(from, none, 1, cx);
    own_record(&mut rows, "drag down", 1, &input, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    own_record(&mut rows, "drag down", 2, &input, cx);
    up(to, none, 1, cx);
    own_record(&mut rows, "drag down", 3, &input, cx);

    let input = open(0, cx);
    let (from, to) = (own_gutter(&input, 3, cx), own_gutter(&input, 1, cx));
    down(from, none, 1, cx);
    cx.simulate_mouse_move(to, MouseButton::Left, none);
    up(to, none, 1, cx);
    own_record(&mut rows, "drag up", 1, &input, cx);

    let input = open(TEXT.find("main").unwrap() + 2, cx);
    let at = own_gutter(&input, 3, cx);
    down(at, shift, 1, cx);
    up(at, shift, 1, cx);
    own_record(&mut rows, "shift click", 1, &input, cx);

    for clicks in [2, 3] {
        let case = format!("{clicks} clicks");
        let input = open(0, cx);
        let at = own_gutter(&input, 2, cx);
        for count in 1..=clicks {
            down(at, none, count, cx);
            up(at, none, count, cx);
            own_record(&mut rows, &case, count, &input, cx);
        }
    }
    rows
}

#[gpui::test]
fn own_gutter_presses_match_the_golden_file(cx: &mut TestAppContext) {
    golden_files::check(SET, NAME, &run_own(cx));
}
