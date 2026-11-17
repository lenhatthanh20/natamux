//! A button that shows one icon.

use gpui::{App, ElementId, IntoElement, ParentElement, Pixels, RenderOnce, SharedString, Window};

use crate::button_like::ButtonLike;
use crate::button_setters::forward_button_setters;
use crate::color::Color;
use crate::icon::{Icon, IconSize};
use crate::icon_name::IconName;
use crate::spacing::DynamicSpacing;
use crate::traits::{FixedWidth, VisibleOnHover};

#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub enum IconButtonShape {
    /// As wide and as high as the icon plus a little room on each side.
    Square,
    /// The button's own height and padding (the default).
    Wide,
}

#[derive(IntoElement)]
pub struct IconButton {
    base: ButtonLike,
    glyph: IconName,
    glyph_size: IconSize,
    tint: Color,
    tint_when_on: Option<Color>,
    square: bool,
}

forward_button_setters!(IconButton, base);

impl IconButton {
    pub fn new(id: impl Into<ElementId>, icon: IconName) -> Self {
        Self {
            base: ButtonLike::new(id),
            glyph: icon,
            glyph_size: IconSize::default(),
            tint: Color::Default,
            tint_when_on: None,
            square: false,
        }
    }

    pub fn shape(self, shape: IconButtonShape) -> Self {
        Self {
            square: shape == IconButtonShape::Square,
            ..self
        }
    }

    pub fn icon_size(self, icon_size: IconSize) -> Self {
        Self {
            glyph_size: icon_size,
            ..self
        }
    }

    pub fn icon_color(self, icon_color: Color) -> Self {
        Self {
            tint: icon_color,
            ..self
        }
    }

    pub fn selected_icon_color(self, color: impl Into<Option<Color>>) -> Self {
        Self {
            tint_when_on: color.into(),
            ..self
        }
    }

    pub fn aria_label(self, label: impl Into<SharedString>) -> Self {
        self.on_inner(|b| b.aria_label(label))
    }

    pub fn aria_expanded(self, expanded: bool) -> Self {
        self.on_inner(|b| b.aria_expanded(expanded))
    }
}

impl VisibleOnHover for IconButton {
    fn visible_on_hover(self, group_name: impl Into<SharedString>) -> Self {
        self.on_inner(|b| b.visible_on_hover(group_name))
    }
}

/// The icon colour: grey while off, the selected colour (the accent unless set) while selected,
/// else the icon's own colour.
fn glyph_colour(
    disabled: bool,
    selected: bool,
    tint: Color,
    tint_when_on: Option<Color>,
) -> Color {
    match (disabled, selected) {
        (true, _) => Color::Disabled,
        (false, true) => tint_when_on.unwrap_or(Color::Selected),
        (false, false) => tint,
    }
}

/// The side of a square button: the icon and the same room on each side, 2 px at UI size 16.
/// An indicator gets no room; a custom size gets its own size as room.
fn square_side(size: IconSize, window: &Window, cx: &App) -> Pixels {
    let rem = window.rem_size();
    let room = match size {
        IconSize::Custom(length) => length.to_pixels(rem),
        IconSize::Indicator => DynamicSpacing::Base00.px(cx),
        _ => DynamicSpacing::Base02.px(cx),
    };
    size.rems() * rem + room * 2.
}

impl RenderOnce for IconButton {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let colour = glyph_colour(
            self.base.setup.disabled,
            self.base.setup.is_on(),
            self.tint,
            self.tint_when_on,
        );
        let mut base = self.base;
        if self.square {
            let side = square_side(self.glyph_size, window, cx);
            base = base.width(side).height(side.into());
        }
        base.child(Icon::new(self.glyph).size(self.glyph_size).color(colour))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::button_like::ButtonCommon;
    use crate::traits::{Disableable, Toggleable};

    #[test]
    fn the_icon_colour_follows_off_then_selected_then_the_icon_colour() {
        let muted = Color::Muted;
        assert_eq!(glyph_colour(true, true, muted, Some(Color::Accent)), Color::Disabled);
        assert_eq!(glyph_colour(false, true, muted, Some(Color::Accent)), Color::Accent);
        assert_eq!(glyph_colour(false, true, muted, None), Color::Selected);
        assert_eq!(glyph_colour(false, false, muted, Some(Color::Accent)), muted);
    }

    #[test]
    fn an_icon_button_is_wide_unless_made_square() {
        let button = IconButton::new("i", IconName::Close);
        assert!(!button.square);
        let button = button.shape(IconButtonShape::Square);
        assert!(button.square);
        assert!(!button.shape(IconButtonShape::Wide).square);
    }

    #[test]
    fn an_icon_button_hands_its_box_setters_to_the_box() {
        let button = IconButton::new("i", IconName::Close)
            .disabled(true)
            .toggle_state(true);
        assert!(button.base.setup.disabled);
        assert!(button.base.setup.is_on());
        assert_eq!(button.id(), &ElementId::from("i"));
    }
}
