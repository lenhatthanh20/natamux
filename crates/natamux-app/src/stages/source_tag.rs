//! The small "Built-in" / "Custom" tag next to a stage name, and the one look every custom mark
//! uses (the tag in the stage list, the preview ring and legend), so a custom stage stands out the
//! same way everywhere.

use gpui::{AnyElement, BoxShadow, FontWeight, Hsla, point};
use ui::prelude::*;

/// The fill of a custom mark: the theme's accent, lit.
pub fn custom_fill(accent: Hsla) -> Hsla {
    accent.opacity(0.22)
}

pub fn custom_border(accent: Hsla) -> Hsla {
    accent.opacity(0.55)
}

/// A soft accent glow, as a lit node tile on the Progress chart.
pub fn custom_glow(accent: Hsla) -> Vec<BoxShadow> {
    vec![BoxShadow {
        color: accent.opacity(0.35),
        offset: point(px(0.), px(0.)),
        blur_radius: px(8.),
        spread_radius: px(0.),
        inset: false,
    }]
}

struct TagStyle {
    text: &'static str,
    color: Color,
    weight: FontWeight,
    bg: Hsla,
    border: Option<Hsla>,
}

fn tag_style(custom: bool, accent: Hsla, hover: Hsla) -> TagStyle {
    if custom {
        TagStyle {
            text: "Custom",
            color: Color::Accent,
            weight: FontWeight::SEMIBOLD,
            bg: custom_fill(accent),
            border: Some(custom_border(accent)),
        }
    } else {
        TagStyle {
            text: "Built-in",
            color: Color::Muted,
            weight: FontWeight::MEDIUM,
            bg: hover,
            border: None,
        }
    }
}

/// 16 px tall, 10.5 px. Custom is semibold accent text on a lit accent fill with an accent
/// border; built-in is muted text on the hover colour.
pub fn source_tag(custom: bool, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let style = tag_style(custom, colors.text_accent, colors.ghost_element_hover);
    div()
        .flex_none()
        .flex()
        .items_center()
        .h(rems_from_px(16.0_f32))
        .px(rems_from_px(5.0_f32))
        .rounded(rems_from_px(4.0_f32))
        .bg(style.bg)
        .when_some(style.border, |this, border| {
            this.border_1().border_color(border)
        })
        .child(
            Label::new(style.text)
                .size(LabelSize::Custom(rems_from_px(10.5_f32)))
                .weight(style.weight)
                .color(style.color),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::components::theme_contrast::{parse_hex, theme_style};
    use serde_json::Value;

    const FAMILY: &str = include_str!("../../assets/themes/natamux.json");

    /// A colour of the shipped dark theme, so the test holds no colour literal.
    fn theme_colour(key: &str) -> Hsla {
        let family: Value = serde_json::from_str(FAMILY).expect("theme file is JSON");
        let style = theme_style(&family, "Natamux Dark").expect("dark theme");
        let hex = style[key].as_str().expect("colour key");
        let c = parse_hex(hex).expect("hex colour");
        let unit = |v: u8| f32::from(v) / 255.;
        gpui::Rgba {
            r: unit(c.r),
            g: unit(c.g),
            b: unit(c.b),
            a: c.a as f32,
        }
        .into()
    }

    #[test]
    fn custom_marks_are_the_accent_lit() {
        let accent = theme_colour("text.accent");
        assert_eq!(custom_fill(accent), accent.opacity(0.22));
        assert_eq!(custom_border(accent), accent.opacity(0.55));
        let glow = custom_glow(accent);
        assert_eq!(glow.len(), 1);
        assert_eq!(glow[0].color, accent.opacity(0.35));
    }

    #[test]
    fn custom_tag_is_lit_and_built_in_tag_is_not() {
        let accent = theme_colour("text.accent");
        let hover = theme_colour("ghost_element.hover");
        let custom = tag_style(true, accent, hover);
        assert_eq!(custom.text, "Custom");
        assert_eq!(custom.bg, custom_fill(accent));
        assert_eq!(custom.border, Some(custom_border(accent)));
        assert_eq!(custom.weight, FontWeight::SEMIBOLD);
        let built_in = tag_style(false, accent, hover);
        assert_eq!(built_in.text, "Built-in");
        assert_eq!(built_in.bg, hover);
        assert_eq!(built_in.border, None);
        assert_eq!(built_in.weight, FontWeight::MEDIUM);
    }
}
