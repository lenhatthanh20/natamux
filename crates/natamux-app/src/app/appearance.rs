//! Theme, UI size and terminal font as Zed settings: the app rebuilds its user settings from
//! `view.json` and sets them again, so a change applies at once (`lib/theme.ts`,
//! `lib/ui-size.ts`, `terminal-font.ts` of the web).

use gpui::{App, Font, FontFeatures, Global, Pixels, SharedString, UpdateGlobal, font, px};
use settings::SettingsStore;

use super::appearance_rules::{
    BASE_BUFFER_FONT_SIZE, BASE_UI_FONT_SIZE, ThemeSetting, parse_theme, parse_ui_size, scaled,
    shows_scrollbars, user_settings,
};
use super::options::ThemeMode;
use super::theme_catalog::ThemeKind;
use super::view_state::ViewState;
use crate::terminals::terminal_font::{
    FontSource, ResolvedFont, parse_terminal_font, terminal_font,
};

/// What the settings come from besides `view.json`: the `--theme` and `--theme-name` flags, and
/// the state of a `--shot`.
#[derive(Clone, Copy)]
pub struct Inputs {
    pub flag: Option<ThemeMode>,
    pub theme_name: Option<(&'static str, ThemeKind)>,
    pub shot: Option<&'static str>,
}

impl Global for Inputs {}

/// The fonts and sizes of the own terminal and text views (docs/plan-mit-license.md 6.1), from
/// the same inputs as the settings JSON, so they equal what Zed makes of it.
// Only the parity test reads it until the own terminal view does (S2.4).
#[cfg_attr(not(test), allow(dead_code))]
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub ui_font: Font,
    pub ui_font_size: Pixels,
    pub buffer_font: Font,
    pub buffer_font_size: Pixels,
    /// A factor of the font size.
    pub buffer_line_height: f32,
    /// The font the user picked for terminals, else None: they use the buffer font and size.
    pub terminal_font: Option<(SharedString, Pixels)>,
    pub hide_scrollbars: bool,
}

impl Global for Appearance {}

/// Zed's "comfortable" line height.
const BUFFER_LINE_HEIGHT: f32 = 1.618;

/// A font size as Zed keeps it: from the JSON's number, within 6 to 100.
fn font_size(size: f64) -> Pixels {
    px((size as f32).clamp(6., 100.))
}

fn appearance(zoom: f32, terminal: &ResolvedFont, shot: Option<&str>) -> Appearance {
    Appearance {
        ui_font: Font {
            features: FontFeatures::disable_ligatures(),
            ..font(".ZedSans")
        },
        ui_font_size: font_size(scaled(BASE_UI_FONT_SIZE, zoom)),
        buffer_font: font(".ZedMono"),
        buffer_font_size: font_size(scaled(BASE_BUFFER_FONT_SIZE, zoom)),
        buffer_line_height: BUFFER_LINE_HEIGHT,
        terminal_font: (terminal.source == FontSource::Custom).then(|| {
            (
                SharedString::from(terminal.family.clone()),
                px(terminal.size.clamp(6., 100.)),
            )
        }),
        hide_scrollbars: shot.is_some_and(|state| !shows_scrollbars(state)),
    }
}

pub fn init(inputs: Inputs, cx: &mut App) {
    cx.set_global(inputs);
}

/// The flags win over the stored theme, as the shots need: `--theme-name` shows that theme in
/// its own look.
pub fn apply(view: &ViewState, cx: &mut App) {
    let inputs = cx.try_global::<Inputs>().copied();
    let (flag, theme_name, shot) = inputs.map_or((None, None, None), |inputs| {
        (inputs.flag, inputs.theme_name, inputs.shot)
    });
    let mode = match (theme_name, flag) {
        (Some((_, ThemeKind::Light)), _) | (None, Some(ThemeMode::Light)) => "light",
        (Some((_, ThemeKind::Dark)), _) | (None, Some(ThemeMode::Dark)) => "dark",
        (None, None) => parse_theme(&view.theme).name(),
    };
    let names = match theme_name {
        Some((name, kind)) => view.themes.with(kind, name),
        None => view.themes,
    };
    let custom = view.terminal_font.as_ref().and_then(parse_terminal_font);
    let font = terminal_font(custom.as_ref());
    let theme = ThemeSetting { mode, names };
    let zoom = parse_ui_size(&view.ui_size).zoom();
    let content = user_settings(theme, zoom, &font, shot);
    SettingsStore::update_global(cx, |store, cx| {
        let _ = store.set_user_settings(&content, cx);
    });
    cx.set_global(appearance(zoom, &font, shot));
}

#[cfg(test)]
#[path = "appearance_tests.rs"]
mod tests;
