//! Theme, UI size and terminal font as Zed settings: the app rebuilds its user settings from
//! `view.json` and sets them again, so a change applies at once (`lib/theme.ts`,
//! `lib/ui-size.ts`, `terminal-font.ts` of the web).

use gpui::{App, Global, UpdateGlobal};
use settings::SettingsStore;

use super::appearance_rules::{parse_theme, parse_ui_size, user_settings};
use super::options::ThemeMode;
use super::view_state::ViewState;
use crate::terminals::ghostty_font::read_ghostty_font;
use crate::terminals::terminal_font::{TerminalFont, parse_terminal_font, terminal_font};

/// What the settings come from besides `view.json`: the `--theme` flag and Ghostty's font, read
/// once at start.
struct Inputs {
    flag: Option<ThemeMode>,
    ghostty: Option<TerminalFont>,
}

impl Global for Inputs {}

pub fn init(flag: Option<ThemeMode>, cx: &mut App) {
    cx.set_global(Inputs {
        flag,
        ghostty: read_ghostty_font(),
    });
}

/// Ghostty's font, for the terminal font dialog's text.
pub fn ghostty_font(cx: &App) -> Option<TerminalFont> {
    cx.try_global::<Inputs>()
        .and_then(|inputs| inputs.ghostty.clone())
}

/// The flag wins over the stored theme, as the shots need.
pub fn apply(view: &ViewState, cx: &mut App) {
    let (flag, ghostty) = cx
        .try_global::<Inputs>()
        .map(|inputs| (inputs.flag, inputs.ghostty.clone()))
        .unwrap_or((None, None));
    let mode = match flag {
        Some(ThemeMode::Light) => "light",
        Some(ThemeMode::Dark) => "dark",
        None => parse_theme(&view.theme).name(),
    };
    let custom = view.terminal_font.as_ref().and_then(parse_terminal_font);
    let font = terminal_font(custom.as_ref(), ghostty.as_ref());
    let content = user_settings(mode, parse_ui_size(&view.ui_size).zoom(), &font);
    SettingsStore::update_global(cx, |store, cx| {
        let _ = store.set_user_settings(&content, cx);
    });
}
