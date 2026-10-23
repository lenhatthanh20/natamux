//! The icon of a file in the Files tab and the Changes tab. Until the Material Icon Theme SVGs
//! are bundled, each name of `file_icons.rs` maps to one of Zed's file icons; this table is the
//! one place to change when the SVGs come.

use gpui::App;
use ui::prelude::*;

use super::file_icons::file_icon_name;

/// Names that are not listed show the plain file icon.
const STAND_INS: &[(&str, IconName)] = &[
    ("markdown", IconName::FileMarkdown),
    ("mdx", IconName::FileMarkdown),
    ("readme", IconName::FileMarkdown),
    ("rust", IconName::FileRust),
    ("toml", IconName::FileToml),
    ("toml_light", IconName::FileToml),
    ("git", IconName::FileGit),
    ("lock", IconName::FileLock),
    ("key", IconName::FileLock),
    ("certificate", IconName::FileLock),
    ("document", IconName::FileDoc),
    ("pdf", IconName::FileDoc),
    ("changelog", IconName::FileDoc),
    ("license", IconName::FileDoc),
    ("log", IconName::FileDoc),
    ("c", IconName::FileCode),
    ("cpp", IconName::FileCode),
    ("csharp", IconName::FileCode),
    ("css", IconName::FileCode),
    ("dart", IconName::FileCode),
    ("elixir", IconName::FileCode),
    ("go", IconName::FileCode),
    ("graphql", IconName::FileCode),
    ("h", IconName::FileCode),
    ("hpp", IconName::FileCode),
    ("html", IconName::FileCode),
    ("java", IconName::FileCode),
    ("javascript", IconName::FileCode),
    ("json", IconName::FileCode),
    ("kotlin", IconName::FileCode),
    ("less", IconName::FileCode),
    ("lua", IconName::FileCode),
    ("php", IconName::FileCode),
    ("python", IconName::FileCode),
    ("react", IconName::FileCode),
    ("react_ts", IconName::FileCode),
    ("ruby", IconName::FileCode),
    ("sass", IconName::FileCode),
    ("svelte", IconName::FileCode),
    ("swift", IconName::FileCode),
    ("test-js", IconName::FileCode),
    ("test-jsx", IconName::FileCode),
    ("test-ts", IconName::FileCode),
    ("typescript", IconName::FileCode),
    ("typescript-def", IconName::FileCode),
    ("vue", IconName::FileCode),
    ("xml", IconName::FileCode),
    ("yaml", IconName::FileCode),
    ("console", IconName::FileCode),
];

pub fn stand_in(name: &str) -> IconName {
    STAND_INS
        .iter()
        .find(|(key, _)| *key == name)
        .map_or(IconName::File, |(_, icon)| *icon)
}

/// The icon of a path, in the light version when the window is light.
pub fn file_icon(path: &str, cx: &App) -> Icon {
    let light = cx.theme().appearance.is_light();
    Icon::new(stand_in(file_icon_name(path, light))).size(IconSize::XSmall)
}
