//! Theme, UI size and terminal font: the app rebuilds its theme settings from `view.json` and
//! sets them again, so a change applies at once (`lib/theme.ts`, `lib/ui-size.ts`,
//! `terminal-font.ts` of the web).

use gpui::{App, Font, Global, Pixels, SharedString};
use theme::ThemeSettings;

use super::appearance_rules::{
    ThemeChoice, parse_theme, parse_ui_size, shows_scrollbars, theme_settings, zoom_factor,
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

/// The fonts and sizes of the own terminal and text views (docs/plan-mit-license.md 6.1): the
/// fonts and sizes of the theme settings, and the terminal font and the shot rules.
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
    /// A `-scrollbar` shot: the code views show their scrollbar all the time.
    pub always_scrollbars: bool,
    /// The diff's Show whitespace: Zed showed the whitespace of every editor then.
    pub show_whitespace: bool,
}

impl Global for Appearance {}

/// `zoom` scales the picked terminal font, as the zoom keys scale the rest (`zoom_factor`).
fn appearance(
    settings: &ThemeSettings,
    terminal: &ResolvedFont,
    zoom: f32,
    shot: Option<&str>,
) -> Appearance {
    Appearance {
        ui_font: settings.ui_font.clone(),
        ui_font_size: settings.ui_font_size,
        buffer_font: settings.buffer_font.clone(),
        buffer_font_size: settings.buffer_font_size,
        buffer_line_height: settings.line_height(),
        terminal_font: (terminal.source == FontSource::Custom).then(|| {
            (
                SharedString::from(terminal.family.clone()),
                theme::zoomed(terminal.size, zoom),
            )
        }),
        hide_scrollbars: shot.is_some_and(|state| !shows_scrollbars(state)),
        always_scrollbars: shot.is_some_and(shows_scrollbars),
        show_whitespace: false,
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
    let choice = match (theme_name, flag) {
        (Some((_, ThemeKind::Light)), _) | (None, Some(ThemeMode::Light)) => ThemeChoice::Light,
        (Some((_, ThemeKind::Dark)), _) | (None, Some(ThemeMode::Dark)) => ThemeChoice::Dark,
        (None, None) => parse_theme(&view.theme),
    };
    let names = match theme_name {
        Some((name, kind)) => view.themes.with(kind, name),
        None => view.themes,
    };
    let custom = view.terminal_font.as_ref().and_then(parse_terminal_font);
    let font = terminal_font(custom.as_ref());
    let zoom = zoom_factor(view.zoom_level);
    let settings = theme_settings(choice, names, parse_ui_size(&view.ui_size).zoom() * zoom);
    let mut look = appearance(&settings, &font, zoom, shot);
    look.show_whitespace = view.diff.show_whitespace;
    theme::set_appearance(cx, |shown| *shown = settings);
    cx.set_global(look);
}

#[cfg(test)]
#[path = "appearance_tests.rs"]
mod tests;
