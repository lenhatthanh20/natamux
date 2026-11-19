//! The rows of buttons under a text box (T-0352): one gap, and each row wraps before it cuts a
//! label.

use ui::prelude::*;

/// A footer row: the buttons at the end of the row, the main action last.
pub fn end_row(buttons: impl IntoIterator<Item = AnyElement>) -> Div {
    h_flex().flex_wrap().justify_end().gap_2().children(buttons)
}

/// `lead` at its own width, then `main` over the rest. Below `main_min` px, `main` takes a full
/// line under `lead`. `main` must fill its box.
pub fn lead_row(lead: impl IntoElement, main: impl IntoElement, main_min: f32) -> Div {
    h_flex()
        .flex_wrap()
        .gap_2()
        .child(lead)
        .child(div().flex_1().min_w(px(main_min)).child(main))
}

/// Buttons that share the row in equal parts; one alone fills it. Below `min` px each, they wrap
/// to one a line. Each button must fill its box.
pub fn shared_row(buttons: impl IntoIterator<Item = AnyElement>, min: f32) -> Div {
    h_flex().flex_wrap().gap_2().children(
        buttons
            .into_iter()
            .map(|button| div().flex_1().min_w(px(min)).child(button)),
    )
}

#[cfg(test)]
#[path = "button_rows_tests.rs"]
mod tests;
