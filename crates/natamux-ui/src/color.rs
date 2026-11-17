//! The colour names the widgets take for text and icons, each read from the theme when drawn.

use gpui::{App, Hsla};
use theme::ActiveTheme;

/// A theme colour by its use, or a fixed one.
#[derive(Debug, Default, PartialEq, Eq, Clone, Copy)]
pub enum Color {
    #[default]
    Default,
    Accent,
    Conflict,
    Created,
    Custom(Hsla),
    Debugger,
    Deleted,
    Disabled,
    Error,
    Hidden,
    Hint,
    Ignored,
    Info,
    Modified,
    Muted,
    Placeholder,
    Player(u32),
    Selected,
    Success,
    VersionControlAdded,
    VersionControlConflict,
    VersionControlDeleted,
    VersionControlIgnored,
    VersionControlModified,
    Warning,
}

impl Color {
    /// The colour in the current theme.
    pub fn color(&self, cx: &App) -> Hsla {
        let theme = cx.theme();
        let ui = theme.colors();
        let status = theme.status();
        match *self {
            Color::Default => ui.text,
            Color::Muted => ui.text_muted,
            Color::Disabled => ui.text_disabled,
            Color::Placeholder => ui.text_placeholder,
            // Selected text and accent text share one theme colour.
            Color::Accent | Color::Selected => ui.text_accent,
            Color::Debugger => ui.debugger_accent,
            Color::VersionControlAdded => ui.version_control_added,
            Color::VersionControlConflict => ui.version_control_conflict,
            Color::VersionControlDeleted => ui.version_control_deleted,
            Color::VersionControlIgnored => ui.version_control_ignored,
            Color::VersionControlModified => ui.version_control_modified,
            Color::Conflict => status.conflict,
            Color::Created => status.created,
            Color::Deleted => status.deleted,
            Color::Error => status.error,
            Color::Hidden => status.hidden,
            Color::Hint => status.hint,
            Color::Ignored => status.ignored,
            Color::Info => status.info,
            Color::Modified => status.modified,
            Color::Success => status.success,
            Color::Warning => status.warning,
            Color::Player(index) => theme.players().color_for_participant(index).cursor,
            Color::Custom(color) => color,
        }
    }
}

impl From<Hsla> for Color {
    fn from(color: Hsla) -> Self {
        Color::Custom(color)
    }
}
