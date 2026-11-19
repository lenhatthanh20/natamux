//! The button rows keep their order and their edges, and wrap in a narrow box (T-0352).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Bounds, Pixels, TestAppContext, VisualTestContext};
use ui::prelude::*;

use super::{end_row, lead_row, shared_row};
use crate::app::test_views;

const GAP: f32 = 8.;
const HEIGHT: f32 = 22.;

/// The three rows, one under the other, in a box of `width` px.
struct Rows {
    width: f32,
}

/// A button of a fixed size.
fn fixed(name: &'static str, width: f32) -> AnyElement {
    div()
        .flex_none()
        .w(px(width))
        .h(px(HEIGHT))
        .debug_selector(|| name.into())
        .into_any_element()
}

/// A button that fills its box, as a full-width button does.
fn filling(name: &'static str) -> AnyElement {
    div()
        .w_full()
        .h(px(HEIGHT))
        .debug_selector(|| name.into())
        .into_any_element()
}

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .w(px(self.width))
            .child(end_row([fixed("quiet", 60.), fixed("action", 90.)]))
            .child(lead_row(fixed("lead", 80.), filling("main"), 130.))
            .child(shared_row([filling("first"), filling("second")], 100.))
    }
}

fn draw(width: f32, cx: &mut TestAppContext) -> &mut VisualTestContext {
    test_views::init(cx);
    let (_rows, cx) = cx.add_window_view(|_, _| Rows { width });
    cx.run_until_parked();
    cx
}

fn bounds(name: &'static str, cx: &mut VisualTestContext) -> Bounds<Pixels> {
    cx.debug_bounds(name)
        .unwrap_or_else(|| panic!("{name} was not drawn"))
}

#[gpui::test]
fn a_wide_box_holds_each_row_on_one_line(cx: &mut TestAppContext) {
    let width = 300.;
    let cx = draw(width, cx);

    let (quiet, action) = (bounds("quiet", cx), bounds("action", cx));
    assert_eq!(action.right(), px(width), "the main action ends the row");
    assert_eq!(quiet.right(), action.left() - px(GAP));
    assert_eq!(quiet.top(), action.top());

    let (lead, main) = (bounds("lead", cx), bounds("main", cx));
    assert_eq!(lead.left(), px(0.));
    assert_eq!(main.left(), lead.right() + px(GAP));
    assert_eq!(main.right(), px(width), "the main button fills the rest");
    assert_eq!(lead.top(), main.top());

    let (first, second) = (bounds("first", cx), bounds("second", cx));
    assert_eq!(first.left(), px(0.));
    assert_eq!(second.right(), px(width));
    assert_eq!(first.size.width, px((width - GAP) / 2.));
    assert_eq!(first.size.width, second.size.width);
    assert_eq!(first.top(), second.top());
}

#[gpui::test]
fn a_narrow_box_wraps_each_row(cx: &mut TestAppContext) {
    let width = 140.;
    let cx = draw(width, cx);

    // 60 + 8 + 90 does not fit: both stay at the end, the main action under the quiet one.
    let (quiet, action) = (bounds("quiet", cx), bounds("action", cx));
    assert_eq!(quiet.right(), px(width));
    assert_eq!(action.right(), px(width));
    assert!(action.top() >= quiet.bottom());

    // 80 + 8 + 130 does not fit: the main button takes a full line.
    let (lead, main) = (bounds("lead", cx), bounds("main", cx));
    assert!(main.top() >= lead.bottom());
    assert_eq!(main.left(), px(0.));
    assert_eq!(main.size.width, px(width));

    // 100 + 8 + 100 does not fit: one button a line, each full width.
    let (first, second) = (bounds("first", cx), bounds("second", cx));
    assert!(second.top() >= first.bottom());
    assert_eq!(first.size.width, px(width));
    assert_eq!(second.size.width, px(width));
}

#[gpui::test]
fn one_shared_button_fills_the_row(cx: &mut TestAppContext) {
    struct One;
    impl Render for One {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div().w(px(300.)).child(shared_row([filling("only")], 100.))
        }
    }
    test_views::init(cx);
    let (_one, cx) = cx.add_window_view(|_, _| One);
    cx.run_until_parked();

    let only = bounds("only", cx);
    assert_eq!(only.left(), px(0.));
    assert_eq!(only.size.width, px(300.));
}
