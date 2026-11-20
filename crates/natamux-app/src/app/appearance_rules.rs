//! Theme and UI size, and the theme settings the app makes of them (`lib/theme.ts`,
//! `lib/ui-size.ts` of the web).

use theme::{ThemeMode, ThemeSettings};

use crate::app::theme_catalog::ThemeNames;
use crate::components::colors::CLEAR;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [Self::System, Self::Light, Self::Dark];

    /// The name stored in `view.json`.
    pub fn name(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::Light => "light",
            Self::Dark => "dark",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::Light => "Light",
            Self::Dark => "Dark",
        }
    }

    pub fn mode(self) -> ThemeMode {
        match self {
            Self::System => ThemeMode::System,
            Self::Light => ThemeMode::Light,
            Self::Dark => ThemeMode::Dark,
        }
    }
}

/// Anything but `light` or `dark` is `system`.
pub fn parse_theme(raw: &str) -> ThemeChoice {
    match raw {
        "light" => ThemeChoice::Light,
        "dark" => ThemeChoice::Dark,
        _ => ThemeChoice::System,
    }
}

/// True for dark. The system look counts only when the choice is System.
// The theme settings follow the system themselves (`ThemeChoice::mode`); the web's rule is kept
// with its test.
#[cfg_attr(not(test), allow(dead_code))]
pub fn resolve_theme(choice: ThemeChoice, system_is_dark: bool) -> bool {
    match choice {
        ThemeChoice::System => system_is_dark,
        ThemeChoice::Light => false,
        ThemeChoice::Dark => true,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum UiSize {
    Compact,
    #[default]
    Default,
    Large,
}

impl UiSize {
    pub const ALL: [UiSize; 3] = [Self::Compact, Self::Default, Self::Large];

    pub fn name(self) -> &'static str {
        match self {
            Self::Compact => "compact",
            Self::Default => "default",
            Self::Large => "large",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Compact => "Compact",
            Self::Default => "Default",
            Self::Large => "Large",
        }
    }

    /// The body text is 12, 13 or 14px in the web; every size follows this factor.
    pub fn zoom(self) -> f32 {
        match self {
            Self::Compact => 0.92,
            Self::Default => 1.0,
            Self::Large => 1.077,
        }
    }
}

/// Anything but `compact` or `large` is `default`.
pub fn parse_ui_size(raw: &str) -> UiSize {
    match raw {
        "compact" => UiSize::Compact,
        "large" => UiSize::Large,
        _ => UiSize::Default,
    }
}

/// The zoom keys' range (`zoomLevel` in `view.json`): 0.7 to 1.5 of the UI size.
pub const ZOOM_MIN: i32 = -3;
pub const ZOOM_MAX: i32 = 5;

pub fn clamp_zoom_level(level: i32) -> i32 {
    level.clamp(ZOOM_MIN, ZOOM_MAX)
}

/// Steps of 10%: VS Code's 20% steps are too coarse for 13px text.
pub fn zoom_factor(level: i32) -> f32 {
    1.0 + 0.1 * clamp_zoom_level(level) as f32
}

/// The end of the name of the `--shot` states that show the scrollbars.
const SCROLLBAR_STATES: &str = "-scrollbar";

/// Whether a `--shot` state shows the scrollbars; every other shot hides them.
pub fn shows_scrollbars(state: &str) -> bool {
    state.ends_with(SCROLLBAR_STATES)
}

/// The theme settings: `choice` picks the look (a `--theme` flag wins before this), `names` the
/// theme of each look, and the UI and code fonts follow `ui_zoom` (`UiSize::zoom`). The code
/// views' scrollbar track has no border line (T-0294).
pub fn theme_settings(choice: ThemeChoice, names: ThemeNames, ui_zoom: f32) -> ThemeSettings {
    ThemeSettings {
        mode: choice.mode(),
        light_theme: names.light.into(),
        dark_theme: names.dark.into(),
        color_overrides: vec![("scrollbar_track_border".into(), CLEAR)],
        ..ThemeSettings::at_zoom(ui_zoom)
    }
}

#[cfg(test)]
#[path = "appearance_rules_tests.rs"]
mod tests;
