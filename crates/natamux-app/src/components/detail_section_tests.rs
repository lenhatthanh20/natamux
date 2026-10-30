//! A gpui div is a block, so a value in `field` fills the value column; `field_fit` keeps a
//! button as wide as its label (T-0232).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::{field, field_fit};
use crate::app::test_views;

const WIDTH: f32 = 300.;

/// The same 40 px value in a `field` row and in a `field_fit` row.
struct Rows;

impl Render for Rows {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let value = |name: &'static str| {
            div()
                .debug_selector(move || name.into())
                .child(div().size(px(40.)))
        };
        v_flex()
            .w(px(WIDTH))
            .child(field("Block", value("block-value")))
            .child(field_fit("Fit", value("fit-value")))
    }
}

#[gpui::test]
fn a_fit_value_keeps_its_own_width(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_rows, cx) = cx.add_window_view(|_, _| Rows);
    cx.run_until_parked();

    let block = cx
        .debug_bounds("block-value")
        .expect("the field value was not drawn");
    assert_eq!(block.right(), px(WIDTH));

    let fit = cx
        .debug_bounds("fit-value")
        .expect("the field_fit value was not drawn");
    assert_eq!(fit.size.width, px(40.));
    assert_eq!(fit.left(), block.left());
}
