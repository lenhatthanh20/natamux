//! Sizes in rem and the two flex rows. A rem is the UI font size (16 px at the default UI size).

use gpui::{App, Div, Pixels, Rems, Styled, div, px, rems};

/// The px length at 16 px per rem, as rems, so it grows and shrinks with the UI size.
pub fn rems_from_px(px: impl Into<f32>) -> Rems {
    rems(px.into() / 16.)
}

/// A row: children side by side, centred on the cross axis.
pub fn h_flex() -> Div {
    div().flex().flex_row().items_center()
}

/// A column: children one under the other.
pub fn v_flex() -> Div {
    div().flex().flex_col()
}

/// A spacing step, named by its px length at the default UI size. The app has one density, so a
/// step is always that length in rems.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum DynamicSpacing {
    Base00,
    Base01,
    Base02,
    Base03,
    Base04,
    Base06,
    Base08,
    Base12,
    Base16,
    Base20,
    Base24,
    Base32,
    Base40,
    Base48,
}

impl DynamicSpacing {
    fn ratio(self) -> f32 {
        let length: f32 = match self {
            DynamicSpacing::Base00 => 0.,
            DynamicSpacing::Base01 => 1.,
            DynamicSpacing::Base02 => 2.,
            DynamicSpacing::Base03 => 3.,
            DynamicSpacing::Base04 => 4.,
            DynamicSpacing::Base06 => 6.,
            DynamicSpacing::Base08 => 8.,
            DynamicSpacing::Base12 => 12.,
            DynamicSpacing::Base16 => 16.,
            DynamicSpacing::Base20 => 20.,
            DynamicSpacing::Base24 => 24.,
            DynamicSpacing::Base32 => 32.,
            DynamicSpacing::Base40 => 40.,
            DynamicSpacing::Base48 => 48.,
        };
        length / 16.
    }

    /// The step in rems.
    pub fn rems(&self, _cx: &App) -> Rems {
        rems(self.ratio())
    }

    /// The step in px at the current UI font size.
    pub fn px(&self, cx: &App) -> Pixels {
        let ui_font_size: f32 = theme::theme_settings(cx).ui_font_size(cx).into();
        px(ui_font_size * self.ratio())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_step_is_its_name_in_px_at_16_px_per_rem() {
        assert_eq!(DynamicSpacing::Base00.ratio(), 0.);
        assert_eq!(DynamicSpacing::Base04.ratio(), 0.25);
        assert_eq!(DynamicSpacing::Base06.ratio(), 6. / 16.);
        assert_eq!(DynamicSpacing::Base48.ratio(), 3.);
        assert_eq!(rems_from_px(14_f32).0, 0.875);
    }
}
