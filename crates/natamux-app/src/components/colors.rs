//! Natamux's own colours (task states, signal, project colours) read from the active theme.
//! Views use these or `cx.theme()`, never colour literals. Which theme colour a state uses is
//! `theme_contrast::state_status`, and `assets/themes/natamux.json` is checked for contrast.

use gpui::{App, Hsla};
use natamux_proto::tasks::task::TaskState;
use theme::{ActiveTheme, StatusColors};

use super::theme_contrast::{StatusKey, state_status};

/// Text and glyph colour, fill (a chip) and border of one state.
#[derive(Clone, Copy, Debug)]
pub struct StateColors {
    pub fg: Hsla,
    pub bg: Hsla,
    pub border: Hsla,
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
