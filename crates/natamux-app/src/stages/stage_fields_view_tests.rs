#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::inline_field;
use crate::app::test_views;

const WIDE: f32 = 400.;
const NARROW: f32 = 100.;
const CONTROL: f32 = 80.;

/// An inline field with a long label and a fixed-width control, in a box of `width`.
struct Field {
    width: f32,
}

impl Render for Field {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let control = div()
            .w(px(CONTROL))
            .h(px(20.))
            .debug_selector(|| "inline-control".into());
        div().w(px(self.width)).child(inline_field(
            "Pause idle agents after (min)",
            None,
            Some("Help".into()),
            control,
        ))
    }
}

fn control_bounds(width: f32, cx: &mut TestAppContext) -> gpui::Bounds<Pixels> {
    let (_view, cx) = cx.add_window_view(|_, _| Field { width });
    cx.run_until_parked();
    cx.debug_bounds("inline-control")
        .expect("the control was not drawn")
}

#[gpui::test]
fn an_inline_control_sits_at_the_right_of_its_label(cx: &mut TestAppContext) {
    test_views::init(cx);
    let control = control_bounds(WIDE, cx);
    assert_eq!(control.right(), px(WIDE));
    // On the label's row, not under it.
    assert!(control.top() < px(12.));
}

#[gpui::test]
fn an_inline_control_wraps_under_a_label_that_leaves_no_room(cx: &mut TestAppContext) {
    test_views::init(cx);
    let wide = control_bounds(WIDE, cx);
    let narrow = control_bounds(NARROW, cx);
    assert!(narrow.top() >= wide.bottom());
    assert!(narrow.right() <= px(NARROW));
}
