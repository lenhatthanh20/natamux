//! The names of the app's SVG icons. Only icons the app or the kit draws are here; each has a file
//! `assets/icons/<snake_case name>.svg` in the app (the app's asset test checks every name).

use std::sync::{Arc, OnceLock};

macro_rules! icon_names {
    ($($name:ident),* $(,)?) => {
        /// An SVG icon of the app's assets.
        #[derive(Debug, PartialEq, Eq, Hash, Clone, Copy)]
        pub enum IconName {
            $($name,)*
        }

        impl IconName {
            /// Every icon name.
            pub const ALL: &[IconName] = &[$(IconName::$name,)*];

            fn camel_name(self) -> &'static str {
                match self {
                    $(IconName::$name => stringify!($name),)*
                }
            }
        }
    };
}

icon_names!(
    ArrowDown,
    ArrowLeft,
    ArrowRight,
    ArrowUp,
    ArrowUpRight,
    Backspace,
    Bell,
    BellOff,
    Book,
    CaseSensitive,
    Chat,
    Check,
    ChevronDown,
    ChevronDownUp,
    ChevronLeft,
    ChevronRight,
    ChevronUp,
    ChevronUpDown,
    Close,
    Command,
    Control,
    Copy,
    Dash,
    Debug,
    DebugPause,
    Delete,
    DiffSplit,
    DiffUnified,
    Download,
    Ellipsis,
    Escape,
    ExpandVertical,
    Eye,
    EyeOff,
    File,
    FileCode,
    FileDiff,
    FileMarkdown,
    FileTextOutlined,
    Filter,
    FoldVertical,
    Folder,
    FolderAdd,
    FontSize,
    GitBranch,
    GitGraph,
    Image,
    Info,
    Keyboard,
    ListTodo,
    ListTree,
    LoadCircle,
    MagnifyingGlass,
    Maximize,
    Menu,
    Minimize,
    Option,
    PageDown,
    PageUp,
    Paperclip,
    Pencil,
    Person,
    Pin,
    PlayFilled,
    PlayOutlined,
    Plus,
    PullRequest,
    Regex,
    Return,
    RotateCcw,
    RotateCw,
    Settings,
    Shift,
    Space,
    Sparkle,
    Split,
    Stop,
    Tab,
    Table,
    Terminal,
    TerminalAlt,
    TextWrap,
    ToolHammer,
    Trash,
    Undo,
    Unpin,
    Warning,
    XCircle,
);

impl IconName {
    /// The asset path: `icons/` + the name in snake case + `.svg`.
    pub fn path(&self) -> Arc<str> {
        format!("icons/{}.svg", snake_case(self.camel_name())).into()
    }

    /// The asset path as a cheap clone: built once per icon.
    pub(crate) fn shared_path(self) -> gpui::SharedString {
        static PATHS: OnceLock<Vec<gpui::SharedString>> = OnceLock::new();
        let all = PATHS.get_or_init(|| {
            IconName::ALL
                .iter()
                .map(|name| gpui::SharedString::from(name.path()))
                .collect()
        });
        all.get(self as usize)
            .cloned()
            .unwrap_or_else(|| gpui::SharedString::from(self.path()))
    }
}

/// `TerminalAlt` -> `terminal_alt`. The names have no digits or runs of capitals.
fn snake_case(camel: &str) -> String {
    let mut out = String::with_capacity(camel.len() + 4);
    for (index, letter) in camel.chars().enumerate() {
        if letter.is_ascii_uppercase() && index > 0 {
            out.push('_');
        }
        out.push(letter.to_ascii_lowercase());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_path_equals_path_for_every_name() {
        for name in IconName::ALL {
            assert_eq!(&*name.shared_path(), &*name.path());
        }
    }

    #[test]
    fn paths_are_snake_case_svg_files() {
        assert_eq!(&*IconName::Check.path(), "icons/check.svg");
        assert_eq!(&*IconName::TerminalAlt.path(), "icons/terminal_alt.svg");
        assert_eq!(&*IconName::XCircle.path(), "icons/x_circle.svg");
        assert_eq!(
            &*IconName::ChevronDownUp.path(),
            "icons/chevron_down_up.svg"
        );
        assert_eq!(IconName::ALL.len(), 88);
    }
}
