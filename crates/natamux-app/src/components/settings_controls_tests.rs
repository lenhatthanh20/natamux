#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::SheetField;
use crate::app::test_views;

const WIDE: f32 = 400.;

/// A sheet field with a 24 px control, in a box of `WIDE`.
struct Field;

impl Render for Field {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let control = div()
            .w_full()
            .h(px(24.))
            .debug_selector(|| "field-control".into());
        div()
            .w(px(WIDE))
            .debug_selector(|| "field-box".into())
            .child(SheetField::new("Name", control).error(Some("Wrong")))
    }
}

// Moved from the old details pane's `labelled` test: the label sits above the control, and the
// control takes the full width of the field.
#[gpui::test]
fn a_sheet_field_puts_its_control_under_the_label_at_full_width(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| Field);
    cx.run_until_parked();
    let field = cx.debug_bounds("field-box").expect("no field");
    let control = cx.debug_bounds("field-control").expect("no control");
    assert_eq!(control.size.width, px(WIDE));
    assert_eq!(control.size.height, px(24.));
    // A 12 px label line, then 4 px.
    assert!(control.top() >= field.top() + px(16.));
}
