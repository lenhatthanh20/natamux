#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::{Mark, labelled};
use crate::app::test_views;

const WIDE: f32 = 400.;

/// A labelled field with a changed mark and a 24 px control, in a box of `WIDE`.
struct Labelled;

impl Render for Labelled {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let mark = Mark {
            dot: div().size(px(6.)).into_any_element(),
            reset: Some(
                div()
                    .size(px(20.))
                    .debug_selector(|| "field-reset".into())
                    .into_any_element(),
            ),
        };
        let control = div()
            .w_full()
            .h(px(24.))
            .debug_selector(|| "field-control".into());
        div()
            .w(px(WIDE))
            .child(labelled("Name", Some(mark), Some("Help".into()), control))
    }
}

#[gpui::test]
fn a_field_has_a_20_px_label_row_with_reset_at_its_end_and_the_control_under_it(
    cx: &mut TestAppContext,
) {
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| Labelled);
    cx.run_until_parked();
    let reset = cx.debug_bounds("field-reset").expect("no reset");
    let control = cx.debug_bounds("field-control").expect("no control");
    assert_eq!(reset.right(), px(WIDE));
    assert_eq!(reset.top(), px(0.));
    // Label row 20 px, then 3 px.
    assert_eq!(control.top(), px(23.));
    assert_eq!(control.size.width, px(WIDE));
}
