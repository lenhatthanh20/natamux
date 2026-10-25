//! The app's icons, by the Lucide names the web uses (the ported rules keep those names). Zed's
//! own icon stands in where one fits, so the app looks like Zed; the Lucide SVGs with no Zed twin
//! stay in `assets/icons/lucide`.

use gpui::SharedString;
use ui::IconName;

/// Zed's icon for a Lucide name, `None` where Zed has none that fits.
pub fn zed_icon(name: &str) -> Option<IconName> {
    let icon = match name {
        "arrow-down" => IconName::ArrowDown,
        "arrow-left" => IconName::ArrowLeft,
        "arrow-up" | "arrow-up-from-line" => IconName::ArrowUp,
        "ban" => IconName::XCircle,
        "bell" => IconName::Bell,
        "bell-off" => IconName::BellOff,
        "case-sensitive" => IconName::CaseSensitive,
        "check" | "circle-check" => IconName::Check,
        "chevron-down" => IconName::ChevronDown,
        "chevron-left" => IconName::ChevronLeft,
        "chevron-right" => IconName::ChevronRight,
        "chevron-up" => IconName::ChevronUp,
        "circle-alert" | "triangle-alert" => IconName::Warning,
        "circle-pause" => IconName::DebugPause,
        "circle-play" => IconName::PlayOutlined,
        "circle-user" => IconName::Person,
        "copy" => IconName::Copy,
        "download" => IconName::Download,
        "ellipsis" => IconName::Ellipsis,
        "external-link" => IconName::ArrowUpRight,
        "eye" => IconName::Eye,
        "eye-off" => IconName::EyeOff,
        "file-diff" => IconName::FileDiff,
        "file-code" => IconName::FileCode,
        "file-text" => IconName::FileTextOutlined,
        "folder-plus" => IconName::FolderAdd,
        "git-branch" => IconName::GitBranch,
        "git-graph" => IconName::GitGraph,
        "git-pull-request" => IconName::PullRequest,
        "info" => IconName::Info,
        "keyboard" => IconName::Keyboard,
        "list-filter" => IconName::Filter,
        "list-todo" => IconName::ListTodo,
        "list-tree" => IconName::ListTree,
        "loader" | "loader-circle" => IconName::LoadCircle,
        "maximize-2" => IconName::Maximize,
        "minimize-2" => IconName::Minimize,
        "menu" => IconName::Menu,
        "minus" => IconName::Dash,
        "paperclip" => IconName::Paperclip,
        "pencil" => IconName::Pencil,
        "pin" => IconName::Pin,
        "pin-off" => IconName::Unpin,
        "play" => IconName::PlayFilled,
        "plus" => IconName::Plus,
        "regex" => IconName::Regex,
        "rotate-ccw" => IconName::RotateCcw,
        "search" => IconName::MagnifyingGlass,
        "settings" => IconName::Settings,
        "sparkles" => IconName::Sparkle,
        "square-split-horizontal" => IconName::Split,
        "square-terminal" => IconName::TerminalAlt,
        "terminal" => IconName::Terminal,
        "trash" => IconName::Trash,
        "undo-2" => IconName::Undo,
        "x" => IconName::Close,
        _ => return None,
    };
    Some(icon)
}

/// The asset path of an icon: Zed's, or the app's Lucide SVG.
pub fn icon_path(name: &str) -> SharedString {
    match zed_icon(name) {
        Some(icon) => icon.path().to_string().into(),
        None => format!("icons/lucide/{name}.svg").into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::asset_table;

    /// The Lucide icons Zed has no twin for.
    const KEPT: [&str; 15] = [
        "bot",
        "circle-dot",
        "columns-3",
        "rows-3",
        "copy-plus",
        "diamond",
        "folder-git-2",
        "folder-minus",
        "layout-grid",
        "list-checks",
        "list-plus",
        "panel-left",
        "panel-right",
        "save",
        "search-x",
    ];

    #[test]
    fn a_lucide_name_with_a_zed_twin_takes_zed_s_icon() {
        assert_eq!(icon_path("search").as_ref(), "icons/magnifying_glass.svg");
        assert_eq!(icon_path("x").as_ref(), IconName::Close.path().as_ref());
    }

    #[test]
    fn the_kept_lucide_icons_have_no_zed_twin_and_ship_with_the_app() {
        for name in KEPT {
            assert!(zed_icon(name).is_none(), "{name}");
            let path = icon_path(name);
            assert!(asset_table::find(&path).is_some(), "{path} missing");
        }
    }
}
