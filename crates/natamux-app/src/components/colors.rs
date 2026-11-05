//! Natamux's own colours (task states, signal, project colours) read from the active theme.
//! Views use these or `cx.theme()`, never colour literals. Which theme colour a state uses is
//! `theme_contrast::state_status`, and every theme of `assets/themes/` is checked for contrast.

use gpui::{App, Hsla};
use natamux_proto::tasks::task::TaskState;
use theme::{ActiveTheme, StatusColors};

use super::theme_contrast::{StatusKey, state_status};

use crate::git::change_rules::StatusTone;

/// Text and glyph colour, fill (a chip) and border of one state.
#[derive(Clone, Copy, Debug)]
pub struct StateColors {
    pub fg: Hsla,
    pub bg: Hsla,
    pub border: Hsla,
}

impl StateColors {
    /// The look of a card or a banner of the state, as Zed's `Banner`: the state colour at 10%,
    /// its border at 20%, so it tints what is under it.
    pub fn tinted(self) -> Self {
        Self {
            fg: self.fg,
            bg: self.fg.opacity(0.1),
            border: self.fg.opacity(0.2),
        }
    }
}

fn pick(status: &StatusColors, key: StatusKey) -> StateColors {
    let (fg, bg, border) = match key {
        StatusKey::Ignored => (
            status.ignored,
            status.ignored_background,
            status.ignored_border,
        ),
        StatusKey::Hint => (status.hint, status.hint_background, status.hint_border),
        StatusKey::Info => (status.info, status.info_background, status.info_border),
        StatusKey::Warning => (
            status.warning,
            status.warning_background,
            status.warning_border,
        ),
        StatusKey::Predictive => (
            status.predictive,
            status.predictive_background,
            status.predictive_border,
        ),
        StatusKey::Conflict => (
            status.conflict,
            status.conflict_background,
            status.conflict_border,
        ),
        StatusKey::Error => (status.error, status.error_background, status.error_border),
        StatusKey::Success => (
            status.success,
            status.success_background,
            status.success_border,
        ),
        StatusKey::Hidden => (
            status.hidden,
            status.hidden_background,
            status.hidden_border,
        ),
    };
    StateColors { fg, bg, border }
}

/// The colours of a task state. A state this build does not know is muted.
pub fn state_colors(state: TaskState, cx: &App) -> StateColors {
    pick(cx.theme().status(), state_status(state))
}

/// "Needs you": the colour of the needs-input state, used for the pill, the edge of a card and
/// the half ring of a quiet agent.
pub fn signal(cx: &App) -> StateColors {
    state_colors(TaskState::NeedsInput, cx)
}

/// The colour of a project, index 0..6 from `project_color_index`. An index past the end wraps.
pub fn project_color(index: usize, cx: &App) -> Hsla {
    cx.theme().accents().color_for_index(index as u32)
}

/// The colour of a lane of the git graph: lane 0 is the accent, so the task's branch matches the
/// rest of the app; the others cycle through the theme's accents by `lane % 6`.
pub fn lane_color(lane: usize, cx: &App) -> Hsla {
    match lane % 6 {
        0 => cx.theme().colors().text_accent,
        index => cx.theme().accents().color_for_index(index as u32),
    }
}

/// The colour of a status letter in a file list (modified, new, deleted or conflict, renamed).
pub fn change_color(tone: StatusTone, cx: &App) -> Hsla {
    let status = cx.theme().status();
    match tone {
        StatusTone::Modified => status.modified,
        StatusTone::Created => status.created,
        StatusTone::Deleted => status.deleted,
        StatusTone::Renamed => status.renamed,
    }
}

/// The two greys of the checkerboard behind an image, so a transparent image shows its holes.
pub fn checker_colors(cx: &App) -> (Hsla, Hsla) {
    let colors = cx.theme().colors();
    (colors.element_background, colors.editor_background)
}

/// The accent of the Settings screen (switches, radios, unsaved dots, blue tiles): the local
/// player's cursor colour, which differs from `text_accent` in the light themes.
pub fn accent(cx: &App) -> Hsla {
    cx.theme().players().local().cursor
}

/// The colour of a Settings tile or avatar: a theme token, filled at `fill_alpha`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TileColor {
    Gray,
    Blue,
    Green,
    Yellow,
    Cyan,
    Violet,
    Red,
}

impl TileColor {
    /// Red is a bit lighter, so an error tile does not shout.
    pub fn fill_alpha(self) -> f32 {
        match self {
            TileColor::Red => 0.18,
            _ => 0.2,
        }
    }
}

/// (fill, glyph) of a tile. Not `StateColors::tinted()`, which is 10% / 20%.
pub fn tile_colors(color: TileColor, cx: &App) -> (Hsla, Hsla) {
    let status = cx.theme().status();
    let fg = match color {
        TileColor::Gray => cx.theme().colors().icon_muted,
        TileColor::Blue => accent(cx),
        TileColor::Green => status.success,
        TileColor::Yellow => status.warning,
        TileColor::Cyan => status.hint,
        TileColor::Violet => status.info,
        TileColor::Red => status.error,
    };
    (fg.opacity(color.fill_alpha()), fg)
}

/// A project's avatar colour from its name, so a project keeps its colour: the sum of the
/// first UTF-16 unit of each char (the mockup's `charCodeAt(0)`), % 5.
pub fn avatar_color(name: &str) -> TileColor {
    const ORDER: [TileColor; 5] = [
        TileColor::Blue,
        TileColor::Green,
        TileColor::Violet,
        TileColor::Cyan,
        TileColor::Yellow,
    ];
    let sum: u64 = name
        .chars()
        .map(|c| u64::from(c.encode_utf16(&mut [0; 2])[0]))
        .sum();
    ORDER[(sum % 5) as usize]
}

/// The letter of a project's avatar: the first char of its name in upper case.
pub fn avatar_letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().collect())
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "colors_tests.rs"]
mod tests;
