//! Theme, UI size and terminal font as Zed settings: the app rebuilds its user settings from
//! `view.json` and sets them again, so a change applies at once (`lib/theme.ts`,
//! `lib/ui-size.ts`, `terminal-font.ts` of the web).

use gpui::{App, Global, UpdateGlobal};
use settings::SettingsStore;

use super::appearance_rules::{ThemeSetting, parse_theme, parse_ui_size, user_settings};
use super::options::ThemeMode;
use super::theme_catalog::ThemeKind;
use super::view_state::ViewState;
use crate::terminals::terminal_font::{parse_terminal_font, terminal_font};

/// What the settings come from besides `view.json`: the `--theme` and `--theme-name` flags, and
/// the state of a `--shot`.
#[derive(Clone, Copy)]
pub struct Inputs {
    pub flag: Option<ThemeMode>,
    pub theme_name: Option<(&'static str, ThemeKind)>,
    pub shot: Option<&'static str>,
}

impl Global for Inputs {}

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
    let content = user_settings(theme, parse_ui_size(&view.ui_size).zoom(), &font, shot);
    SettingsStore::update_global(cx, |store, cx| {
        let _ = store.set_user_settings(&content, cx);
    });
}
