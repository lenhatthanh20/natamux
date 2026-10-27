//! Theme and UI size, and the settings JSON the app gives Zed's settings store
//! (`lib/theme.ts`, `lib/ui-size.ts`, `apply_default_settings` of `main.rs`).

use serde_json::json;

use crate::app::theme_catalog::ThemeNames;
use crate::terminals::terminal_font::{FontSource, ResolvedFont};

// The base the UI size scales. The UI is Zed's default; code (terminal, diff, editors) is 13, not
// Zed's 15, so it sits beside the 13 px rows of the lists (owner, 2026-10-24: 15 looked big).
pub const BASE_UI_FONT_SIZE: f64 = 16.0;
pub const BASE_BUFFER_FONT_SIZE: f64 = 13.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeChoice {
    #[default]
    System,
    Light,
    Dark,
}

impl ThemeChoice {
    pub const ALL: [ThemeChoice; 3] = [Self::System, Self::Light, Self::Dark];

    /// The name stored in `view.json` and written as `theme.mode`.
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
// Zed's theme setting follows the system itself; the web's rule is kept with its test.
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

fn scaled(base: f64, zoom: f32) -> f64 {
    (base * f64::from(zoom) * 10.0).round() / 10.0
}

// The shortest text of the f32, so 13.25 stays 13.25.
fn f64_of(size: f32) -> f64 {
    size.to_string().parse().unwrap_or(f64::from(size))
}

/// The theme part of Zed's settings: `mode` is `ThemeChoice::name` (a `--theme` flag wins before
/// this), and the theme Zed shows in each mode.
#[derive(Clone, Copy, Debug)]
pub struct ThemeSetting<'a> {
    pub mode: &'a str,
    pub names: ThemeNames,
}

/// The user settings JSON for Zed: `ui_zoom` is `UiSize::zoom`. Zed's own fonts; the terminal
/// gets a font only when the user picked one, else Zed gives it the buffer font and size. A
/// `--shot` hides the editors' scrollbars, which Zed shows for a moment after a scroll.
pub fn user_settings(
    theme: ThemeSetting<'_>,
    ui_zoom: f32,
    terminal: &ResolvedFont,
    shot: bool,
) -> String {
    let terminal = if terminal.source == FontSource::Default {
        json!({})
    } else {
        json!({"font_family": terminal.family, "font_size": f64_of(terminal.size)})
    };
    let mut settings = json!({
        "theme": {"mode": theme.mode, "light": theme.names.light, "dark": theme.names.dark},
        "ui_font_family": ".ZedSans",
        "ui_font_size": scaled(BASE_UI_FONT_SIZE, ui_zoom),
        "buffer_font_family": ".ZedMono",
        "buffer_font_size": scaled(BASE_BUFFER_FONT_SIZE, ui_zoom),
        "terminal": terminal,
    });
    if shot {
        settings["scrollbar"] = json!({"show": "never"});
    }
    settings.to_string()
}

#[cfg(test)]
#[path = "appearance_rules_tests.rs"]
mod tests;
