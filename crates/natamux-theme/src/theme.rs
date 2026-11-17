//! A loaded theme and its look.

use std::sync::Arc;

use gpui::{Hsla, SharedString, WindowAppearance, WindowBackgroundAppearance};

use crate::colors::{AccentColors, PlayerColors, StatusColors, ThemeColors};
use crate::syntax::SyntaxTheme;

/// Light or dark: the look of a theme, or of the system.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Light,
    Dark,
}

impl Appearance {
    pub fn is_light(&self) -> bool {
        *self == Appearance::Light
    }
}

impl From<WindowAppearance> for Appearance {
    fn from(value: WindowAppearance) -> Self {
        match value {
            WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
            WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ThemeStyles {
    pub window_background_appearance: WindowBackgroundAppearance,
    pub accents: AccentColors,
    pub colors: ThemeColors,
    pub status: StatusColors,
    pub player: PlayerColors,
    pub syntax: Arc<SyntaxTheme>,
}

#[derive(Clone, Debug)]
pub struct Theme {
    /// The name: the app ships one theme per name.
    pub id: String,
    pub name: SharedString,
    pub appearance: Appearance,
    pub styles: ThemeStyles,
}

impl Theme {
    pub fn colors(&self) -> &ThemeColors {
        &self.styles.colors
    }

    pub fn status(&self) -> &StatusColors {
        &self.styles.status
    }

    pub fn accents(&self) -> &AccentColors {
        &self.styles.accents
    }

    pub fn players(&self) -> &PlayerColors {
        &self.styles.player
    }

    pub fn syntax(&self) -> &Arc<SyntaxTheme> {
        &self.styles.syntax
    }

    pub fn appearance(&self) -> Appearance {
        self.appearance
    }

    pub fn window_background_appearance(&self) -> WindowBackgroundAppearance {
        self.styles.window_background_appearance
    }

    /// `color` with its lightness lowered by the amount of the theme's look, not below 0.
    pub fn darken(&self, color: Hsla, light_amount: f32, dark_amount: f32) -> Hsla {
        let amount = match self.appearance {
            Appearance::Light => light_amount,
            Appearance::Dark => dark_amount,
        };
        Hsla {
            l: (color.l - amount).max(0.),
            ..color
        }
    }
}
