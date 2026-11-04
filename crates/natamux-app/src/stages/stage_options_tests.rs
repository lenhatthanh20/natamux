#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::{Row, render};
use crate::app::test_views;

/// One checkbox row with a note.
struct OptionRow;

impl Render for OptionRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex().w(px(400.)).child(render(
            Row {
                id: "gate-spec".into(),
                label: "Wait for my approval",
                note: Some("This stage is a check by a person."),
                on: false,
                disabled: false,
                mark: None,
                toggle: Box::new(|_, _, _| {}),
            },
            cx,
        ))
    }
}

#[gpui::test]
fn a_note_is_an_info_icon_at_the_end_of_a_one_line_row(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| OptionRow);
    cx.run_until_parked();
    let info = cx
        .debug_bounds("option-info")
        .expect("option-info was not drawn");
    assert_eq!(info.right(), px(400.));
    assert!(info.bottom() <= px(28.));
}
