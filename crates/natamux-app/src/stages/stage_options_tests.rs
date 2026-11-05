#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::TestAppContext;
use ui::prelude::*;

use super::{Row, render};
use crate::app::test_views;
use crate::stages::stage_fields_view::default_mark;

/// One checkbox row at its default, with a note.
struct OptionRow;

impl Render for OptionRow {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mark = default_mark("gate-default-spec", true, |_, _| {}, cx);
        v_flex().w(px(400.)).child(render(
            Row {
                id: "gate-spec".into(),
                label: "Wait for my approval",
                note: Some("Unchecked, the pull request opens when the stage starts."),
                help: None,
                on: false,
                disabled: false,
                mark: Some(mark),
                toggle: Box::new(|_, _, _| {}),
            },
            cx,
        ))
    }
}

#[gpui::test]
fn the_dot_sits_at_the_end_of_a_one_line_checkbox_row(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| OptionRow);
    cx.run_until_parked();
    let dot = cx
        .debug_bounds("option-dot")
        .expect("option-dot was not drawn");
    // No Reset at the default, so the dot is last.
    assert_eq!(dot.right(), px(400.));
    assert!(dot.bottom() <= px(22.));
}
