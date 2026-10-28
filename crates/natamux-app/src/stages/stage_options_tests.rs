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
fn a_note_starts_under_its_checkbox_label(cx: &mut TestAppContext) {
    // The note started under the box, left of the label (T-0215).
    test_views::init(cx);
    let (_view, cx) = cx.add_window_view(|_, _| OptionRow);
    cx.run_until_parked();
    let [label, note] = ["option-label", "option-note"].map(|selector| {
        cx.debug_bounds(selector)
            .unwrap_or_else(|| panic!("{selector} was not drawn"))
    });
    assert_eq!(note.left(), label.left());
}
