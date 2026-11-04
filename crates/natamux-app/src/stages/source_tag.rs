//! The small "Built-in" / "Custom" tag next to a stage name.

use gpui::{AnyElement, FontWeight};
use ui::prelude::*;

/// Custom is accent, built-in is muted, so a custom stage stands out in the details header.
pub fn source_tag(custom: bool, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let (text, color, border) = if custom {
        ("Custom", Color::Accent, colors.text_accent)
    } else {
        ("Built-in", Color::Muted, colors.border_variant)
    };
    div()
        .flex_none()
        .px_1()
        .rounded_sm()
        .border_1()
        .border_color(border)
        .child(
            Label::new(text)
                .size(LabelSize::XSmall)
                .weight(FontWeight::MEDIUM)
                .color(color),
        )
        .into_any_element()
}
