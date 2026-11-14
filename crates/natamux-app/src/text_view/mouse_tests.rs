//! T-MOUSE-1 for the text boxes (plan 7.4, S3.2): the mouse in an editable box and in a read-only
//! one. A press inside a selection, released with no move or after a move before and after 300 ms
//! (a drop that moves the text, or copies it with Alt on macOS and Ctrl elsewhere), and a press
//! after another listener called `prevent_default`. The gutter, the diff line numbers and the
//! double click in the diff are `code_mouse_tests.rs` and `git/diff_mouse_tests.rs`. They use only
//! gpui mouse events and the input method's `bounds_for_range`, so the own input must pass them.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::ops::Range;

use gpui::{
    EntityInputHandler as _, Modifiers, MouseButton, MouseDownEvent, MouseUpEvent, Pixels, Point,
    TestAppContext, VisualTestContext, point,
};
use ui::prelude::*;

use super::text_box::TextBox;
use super::text_box_tests::{focus, pause, press_as};
use crate::app::test_driver::{selection_of, text_of};
use crate::app::test_views;

const TEXT: &str = "hello world foo bar";
const MAC: bool = cfg!(target_os = "macos");

/// An editable area, a read-only view of the same text, and an editable area under a layer that
/// calls `prevent_default` on each press, as a ButtonLike does.
struct Boxes {
    editable: TextBox,
    read_only: TextBox,
    covered: TextBox,
}

impl Render for Boxes {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let boxed = |text_box: &TextBox| div().w(px(400.)).h(px(60.)).child(text_box.clone());
        v_flex()
            .child(boxed(&self.editable))
            .child(boxed(&self.read_only))
            .child(
                div()
                    .relative()
                    .w(px(400.))
                    .h(px(60.))
                    .child(self.covered.clone())
                    .child(div().absolute().top_0().left_0().size_full().on_mouse_down(
                        MouseButton::Left,
                        |_, window, _| {
                            window.prevent_default();
                        },
                    )),
            )
    }
}

fn setup(cx: &mut TestAppContext) -> (Boxes, &mut VisualTestContext) {
    let (view, _, cx) = test_views::init_app(cx, |_, window, cx| {
        cx.new(|cx| Boxes {
            editable: TextBox::area("", (2, 4), window, cx),
            read_only: TextBox::plain(false, window, cx),
            covered: TextBox::area("", (2, 4), window, cx),
        })
    });
    let boxes = view.read_with(cx, |boxes, _| Boxes {
        editable: boxes.editable.clone(),
        read_only: boxes.read_only.clone(),
        covered: boxes.covered.clone(),
    });
    cx.update(|_, cx| crate::app::text_boxes::bind(cx, MAC));
    for text_box in [&boxes.editable, &boxes.read_only, &boxes.covered] {
        cx.update(|window, cx| {
            text_box.set_text(TEXT, window, cx);
        });
    }
    // The read-only view is read-only after its text is in, as the diff's editors are.
    boxes
        .read_only
        .editor()
        .update(cx, |editor, _| editor.set_read_only(true));
    cx.run_until_parked();
    (boxes, cx)
}

/// The window point at the left edge of char `index` (UTF-16 units), or `into` it by `dx` px.
fn at(text_box: &TextBox, index: usize, dx: f32, cx: &mut VisualTestContext) -> Point<Pixels> {
    text_box.editor().update_in(cx, |editor, window, cx| {
        let area = *editor.last_bounds().expect("the box was drawn");
        let rect = editor
            .bounds_for_range(index..index + 1, area, window, cx)
            .expect("the char is shown");
        point(
            rect.origin.x + px(dx),
            rect.origin.y + rect.size.height / 2.,
        )
    })
}

fn down(at: Point<Pixels>, clicks: usize, cx: &mut VisualTestContext) {
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: at,
        modifiers: Modifiers::default(),
        click_count: clicks,
        first_mouse: false,
    });
}

fn up(at: Point<Pixels>, modifiers: Modifiers, cx: &mut VisualTestContext) {
    cx.simulate_event(MouseUpEvent {
        button: MouseButton::Left,
        position: at,
        modifiers,
        click_count: 1,
    });
}

/// Puts the selection on chars `6..11` (`world`) with the keys, and gives the box the focus.
fn select_world(text_box: &TextBox, cx: &mut VisualTestContext) {
    focus(text_box, cx);
    press_as(
        cx,
        "home right right right right right right shift-right shift-right shift-right shift-right shift-right",
        MAC,
    );
    assert_eq!(selection_of(text_box, cx), 6..11);
}

fn drop_modifiers(copy: bool) -> Modifiers {
    Modifiers {
        alt: MAC && copy,
        control: !MAC && copy,
        ..Modifiers::default()
    }
}

#[gpui::test]
fn t_mouse_a_press_inside_the_selection_then_a_release_puts_the_caret_there(
    cx: &mut TestAppContext,
) {
    let (boxes, cx) = setup(cx);
    for (name, text_box) in [
        ("editable", &boxes.editable),
        ("read-only", &boxes.read_only),
    ] {
        select_world(text_box, cx);
        let inside = at(text_box, 8, 1., cx);
        down(inside, 1, cx);
        // The caret does not move on the press: the press may start a drag of the text.
        assert_eq!(selection_of(text_box, cx), 6..11, "{name}: after the press");
        up(inside, Modifiers::default(), cx);
        let caret = selection_of(text_box, cx);
        assert!(
            caret.is_empty() && (8..=9).contains(&caret.start),
            "{name}: {caret:?}"
        );
        assert_eq!(text_of(text_box, cx), TEXT, "{name}");
    }
}

#[gpui::test]
fn t_mouse_a_move_before_300_ms_selects_from_the_press(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    for (name, text_box) in [
        ("editable", &boxes.editable),
        ("read-only", &boxes.read_only),
    ] {
        select_world(text_box, cx);
        let from = at(text_box, 8, 1., cx);
        let to = at(text_box, 15, 0.25, cx);
        down(from, 1, cx);
        cx.simulate_mouse_move(to, MouseButton::Left, Modifiers::default());
        up(to, Modifiers::default(), cx);
        cx.run_until_parked();
        let range = selection_of(text_box, cx);
        assert!(
            (8..=9).contains(&range.start) && range.end == 15,
            "{name}: got {range:?}, want from the press (8 or 9) to 15"
        );
        assert_eq!(text_of(text_box, cx), TEXT, "{name}: no text moved");
    }
}

/// Presses inside `world`, waits 300 ms, moves to the edge of char 15 and lets go with `modifiers`.
/// Returns the text and the selection after.
fn hold_and_drop(
    text_box: &TextBox,
    modifiers: Modifiers,
    cx: &mut VisualTestContext,
) -> (String, Range<usize>) {
    select_world(text_box, cx);
    let from = at(text_box, 8, 1., cx);
    let to = at(text_box, 15, 0.25, cx);
    down(from, 1, cx);
    pause(cx);
    cx.simulate_mouse_move(to, MouseButton::Left, modifiers);
    up(to, modifiers, cx);
    cx.run_until_parked();
    (text_of(text_box, cx), selection_of(text_box, cx))
}

#[gpui::test]
fn t_mouse_a_move_after_300_ms_moves_the_text_of_an_editable_box(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    let (text, selection) = hold_and_drop(&boxes.editable, drop_modifiers(false), cx);
    eprintln!("T-MOUSE-1 editable, drop: {text:?}, selection {selection:?}");
    // `world` leaves its place and goes in at the release point, right after `foo`.
    assert_eq!(text, "hello  fooworld bar");
}

#[gpui::test]
fn t_mouse_a_move_after_300_ms_with_the_copy_key_copies_the_text(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    let (text, selection) = hold_and_drop(&boxes.editable, drop_modifiers(true), cx);
    eprintln!("T-MOUSE-1 editable, copy drop: {text:?}, selection {selection:?}");
    assert_eq!(text, "hello world fooworld bar");
}

#[gpui::test]
fn t_mouse_a_move_after_300_ms_in_a_read_only_view_moves_the_text_today(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    // Record (a bug of today's code, in the log): the read-only box still moves text on a drop.
    for (copy, want) in [
        (false, "hello  fooworld bar"),
        (true, "hello world fooworld bar"),
    ] {
        let (text, selection) = hold_and_drop(&boxes.read_only, drop_modifiers(copy), cx);
        eprintln!("T-MOUSE-1 read-only, copy {copy}: {text:?}, selection {selection:?}");
        assert_eq!(text, want, "copy {copy}");
        cx.update(|window, cx| boxes.read_only.set_text(TEXT, window, cx));
    }
}

#[gpui::test]
fn t_mouse_a_press_after_prevent_default_does_nothing(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    // The covered box: the layer over it calls `prevent_default` first, so the press leaves the
    // caret and the focus where they are.
    focus(&boxes.editable, cx);
    cx.update(|window, cx| boxes.covered.set_text(TEXT, window, cx));
    let point = at(&boxes.covered, 8, 1., cx);
    down(point, 1, cx);
    up(point, Modifiers::default(), cx);
    cx.run_until_parked();
    let focused = cx.update(|window, cx| boxes.covered.is_focused(window, cx));
    let selection = selection_of(&boxes.covered, cx);
    assert!(!focused, "the press focused the covered box");
    assert!(
        !(8..=9).contains(&selection.start),
        "the caret went to the press: {selection:?}"
    );
    // The same press on the free box places the caret and takes the focus.
    let free = at(&boxes.editable, 8, 1., cx);
    down(free, 1, cx);
    up(free, Modifiers::default(), cx);
    cx.run_until_parked();
    let selection = selection_of(&boxes.editable, cx);
    assert!(
        selection.is_empty() && (8..=9).contains(&selection.start),
        "{selection:?}"
    );
}

#[gpui::test]
fn t_mouse_clicks_select_a_word_and_a_line(cx: &mut TestAppContext) {
    let (boxes, cx) = setup(cx);
    for (name, text_box) in [
        ("editable", &boxes.editable),
        ("read-only", &boxes.read_only),
    ] {
        focus(text_box, cx);
        let word = at(text_box, 7, 1., cx);
        for clicks in 1..=2 {
            down(word, clicks, cx);
            up(word, Modifiers::default(), cx);
        }
        assert_eq!(selection_of(text_box, cx), 6..11, "{name}: double click");
        down(word, 3, cx);
        up(word, Modifiers::default(), cx);
        assert_eq!(
            selection_of(text_box, cx),
            0..TEXT.len(),
            "{name}: triple click"
        );
    }
}
