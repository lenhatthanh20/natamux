//! The icon of a file in the Files tab and the Changes tab: Zed's file icons
//! (`icons/file_icons/*.svg` of Zed's assets), picked by the Material Icon Theme name of
//! `file_icons.rs`, as Zed's project panel shows them.

use gpui::{App, SharedString};
use ui::prelude::*;

use super::file_icons::file_icon_name;

/// Zed's file icons that have a Material name of the same spelling.
const SAME_NAME: &[&str] = &[
    "audio",
    "bun",
    "c",
    "cpp",
    "css",
    "dart",
    "database",
    "diff",
    "docker",
    "editorconfig",
    "elixir",
    "eslint",
    "file",
    "font",
    "git",
    "go",
    "graphql",
    "html",
    "image",
    "java",
    "javascript",
    "jupyter",
    "kotlin",
    "lock",
    "lua",
    "php",
    "prettier",
    "python",
    "react",
    "ruby",
    "rust",
    "sass",
    "settings",
    "swift",
    "terraform",
    "toml",
    "typescript",
    "video",
    "vue",
    "yaml",
];

/// Zed's file icon (the file name without `.svg`) for a Material name; `file` when Zed has none
/// near it.
pub fn zed_file_icon(name: &str) -> &'static str {
    if let Some(same) = SAME_NAME.iter().find(|same| **same == name) {
        return same;
    }
    match name {
        "bun_light" => "bun",
        "toml_light" => "toml",
        "react_ts" | "test-jsx" => "react",
        "typescript-def" | "test-ts" | "tsconfig" => "typescript",
        "test-js" => "javascript",
        "go-mod" => "go",
        "jar" => "java",
        "h" => "c",
        "hpp" => "cpp",
        "less" => "css",
        "svg" => "image",
        "markdown" | "mdx" | "readme" | "document" | "pdf" | "license" | "changelog" | "log" => {
            "book"
        }
        "certificate" | "key" => "lock",
        "console" | "powershell" => "terminal",
        "nodejs" | "npm" | "pnpm" | "pnpm_light" | "yarn" => "package",
        "zip" => "archive",
        "tune" => "settings",
        "table" => "database",
        "agent" | "claude" => "ai",
        "csharp" | "json" | "xml" | "makefile" | "proto" | "svelte" | "webassembly" | "babel"
        | "biome" | "vite" | "vitest" | "playwright" | "vagrant" | "gemfile" => "code",
        _ => "file",
    }
}

/// The icon of a path, in the light version's name when the window is light.
pub fn file_icon(path: &str, cx: &App) -> Icon {
    let light = cx.theme().appearance.is_light();
    let stem = zed_file_icon(file_icon_name(path, light));
    Icon::from_path(SharedString::from(format!("icons/file_icons/{stem}.svg")))
        .size(IconSize::XSmall)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::file_icons::KNOWN_ICONS;

    #[test]
    fn every_known_name_has_a_zed_file_icon() {
        // Zed's file icons at the pinned rev (assets/icons/file_icons).
        const ZED: &[&str] = &[
            "ai",
            "archive",
            "audio",
            "book",
            "bun",
            "c",
            "code",
            "cpp",
            "css",
            "dart",
            "database",
            "diff",
            "docker",
            "editorconfig",
            "elixir",
            "eslint",
            "file",
            "font",
            "git",
            "go",
            "graphql",
            "html",
            "image",
            "java",
            "javascript",
            "jupyter",
            "kotlin",
            "lock",
            "lua",
            "package",
            "php",
            "prettier",
            "python",
            "react",
            "ruby",
            "rust",
            "sass",
            "settings",
            "swift",
            "terminal",
            "terraform",
            "toml",
            "typescript",
            "video",
            "vue",
            "yaml",
        ];
        for name in KNOWN_ICONS {
            let stem = zed_file_icon(name);
            assert!(ZED.contains(&stem), "{name} -> {stem}");
            assert!(stem != "file" || *name == "file", "{name} has no near icon");
        }
    }
}
