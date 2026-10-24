//! The look of a tinted button that is off. Zed's `ButtonLike` keeps the tint of a disabled
//! button and dims only a `Button`'s own label, so an Import or Save that is off looked on. The
//! web greys it (`disabled:opacity-50`).

use ui::{ButtonStyle, Color, TintColor};

/// A tinted button that is off shows as a plain filled one.
pub fn tinted_style(tint: TintColor, disabled: bool) -> ButtonStyle {
    if disabled {
        ButtonStyle::Filled
    } else {
        ButtonStyle::Tinted(tint)
    }
}

/// The colour of a label that a `ButtonLike` draws itself.
pub fn label_color(disabled: bool) -> Color {
    if disabled {
        Color::Disabled
    } else {
        Color::Default
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tinted_button_that_is_off_has_no_tint_and_a_dimmed_label() {
        assert!(matches!(
            tinted_style(TintColor::Accent, true),
            ButtonStyle::Filled
        ));
        assert!(matches!(label_color(true), Color::Disabled));
    }

    #[test]
    fn a_tinted_button_that_is_on_keeps_its_tint_and_label() {
        assert!(matches!(
            tinted_style(TintColor::Error, false),
            ButtonStyle::Tinted(TintColor::Error)
        ));
        assert!(matches!(label_color(false), Color::Default));
    }
}
