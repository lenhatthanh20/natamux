//! The small "Built-in" / "Custom" tag next to a stage name.

use gpui::{AnyElement, FontWeight};
use ui::prelude::*;

/// 16 px tall, 10.5 px medium. Custom is accent text on an accent tint, so a custom stage stands
/// out; built-in is muted text on the hover colour.
pub fn source_tag(custom: bool, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let (text, color, bg) = if custom {
        ("Custom", Color::Accent, colors.text_accent.opacity(0.13))
    } else {
        ("Built-in", Color::Muted, colors.ghost_element_hover)
    };
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(rems_from_px(16.0_f32))
        .px(rems_from_px(5.0_f32))
        .rounded(rems_from_px(4.0_f32))
        .bg(bg)
        .child(
            Label::new(text)
                .size(LabelSize::Custom(rems_from_px(10.5_f32)))
                .weight(FontWeight::MEDIUM)
                .color(color),
        )
        .into_any_element()
}
