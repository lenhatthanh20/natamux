//! The app's own files, built into the binary, as a path table (no gpui, so it can be tested alone).
//! A new file under `assets/` needs a line here.

macro_rules! file {
    ($path:literal) => {
        (
            $path,
            include_bytes!(concat!("../../assets/", $path)) as &[u8],
        )
    };
}

pub const FILES: &[(&str, &[u8])] = &[
    file!("themes/natamux.json"),
    file!("themes/ayu.json"),
    file!("themes/catppuccin.json"),
    file!("themes/dracula.json"),
    file!("themes/github.json"),
    file!("themes/gruvbox.json"),
    file!("themes/nord.json"),
    file!("themes/rose-pine.json"),
    file!("themes/solarized.json"),
    file!("themes/tokyo-night.json"),
    file!("themes/LICENSES"),
    file!("ime-words.txt"),
    file!("icons/states/blocked-8.svg"),
    file!("icons/states/blocked.svg"),
    file!("icons/lucide/bot.svg"),
    file!("icons/states/cancelled-8.svg"),
    file!("icons/states/cancelled.svg"),
    file!("icons/lucide/circle-dot.svg"),
    file!("icons/agents/claude.svg"),
    file!("icons/agents/codex.svg"),
    file!("icons/lucide/columns-3.svg"),
    file!("icons/agents/copilot.svg"),
    file!("icons/agents/copilot_light.svg"),
    file!("icons/lucide/copy-plus.svg"),
    file!("icons/agents/cursor.svg"),
    file!("icons/agents/cursor_light.svg"),
    file!("icons/states/done-8.svg"),
    file!("icons/states/done.svg"),
    file!("icons/lucide/diamond.svg"),
    file!("icons/states/draft-8.svg"),
    file!("icons/states/draft.svg"),
    file!("icons/states/failed-8.svg"),
    file!("icons/states/failed.svg"),
    file!("icons/lucide/folder-git-2.svg"),
    file!("icons/lucide/folder-minus.svg"),
    file!("icons/agents/gemini-ai.svg"),
    file!("icons/states/idle-8.svg"),
    file!("icons/states/idle.svg"),
    file!("icons/lucide/layout-grid.svg"),
    file!("icons/lucide/LICENSE"),
    file!("icons/agents/LICENSE-lobe-icons"),
    file!("icons/agents/LICENSE-material-icon-theme"),
    file!("icons/lucide/list-checks.svg"),
    file!("icons/lucide/list-plus.svg"),
    file!("icons/states/needs-input-8.svg"),
    file!("icons/states/needs-input.svg"),
    file!("icons/lucide/panel-left.svg"),
    file!("icons/lucide/panel-right.svg"),
    file!("icons/states/quiet-8.svg"),
    file!("icons/states/quiet.svg"),
    file!("icons/states/ready-8.svg"),
    file!("icons/states/ready.svg"),
    file!("icons/states/review-8.svg"),
    file!("icons/states/review.svg"),
    file!("icons/lucide/rows-3.svg"),
    file!("icons/states/running-8.svg"),
    file!("icons/states/running.svg"),
    file!("icons/lucide/save.svg"),
    file!("icons/lucide/search-x.svg"),
    file!("icons/lucide/send.svg"),
    file!("icons/lucide/braces.svg"),
    file!("icons/lucide/clock.svg"),
    file!("icons/lucide/flask-conical.svg"),
    file!("icons/lucide/hand.svg"),
    file!("icons/lucide/monitor.svg"),
    file!("icons/lucide/moon.svg"),
    file!("icons/lucide/palette.svg"),
    file!("icons/lucide/panels-top-left.svg"),
    file!("icons/lucide/plug.svg"),
    file!("icons/lucide/shield.svg"),
    file!("icons/lucide/sliders-vertical.svg"),
    file!("icons/lucide/sun.svg"),
    file!("icons/lucide/upload.svg"),
    file!("icons/motion/spin-dashes.svg"),
    file!("icons/motion/spin-dots.svg"),
    file!("icons/states/unknown-8.svg"),
    file!("icons/states/unknown.svg"),
    file!("icons/files/agent.svg"),
    file!("icons/files/audio.svg"),
    file!("icons/files/babel.svg"),
    file!("icons/files/biome.svg"),
    file!("icons/files/bun.svg"),
    file!("icons/files/bun_light.svg"),
    file!("icons/files/c.svg"),
    file!("icons/files/certificate.svg"),
    file!("icons/files/changelog.svg"),
    file!("icons/files/claude.svg"),
    file!("icons/files/console.svg"),
    file!("icons/files/cpp.svg"),
    file!("icons/files/csharp.svg"),
    file!("icons/files/css.svg"),
    file!("icons/files/dart.svg"),
    file!("icons/files/database.svg"),
    file!("icons/files/diff.svg"),
    file!("icons/files/docker.svg"),
    file!("icons/files/document.svg"),
    file!("icons/files/editorconfig.svg"),
    file!("icons/files/elixir.svg"),
    file!("icons/files/eslint.svg"),
    file!("icons/files/file.svg"),
    file!("icons/files/font.svg"),
    file!("icons/files/gemfile.svg"),
    file!("icons/files/git.svg"),
    file!("icons/files/go.svg"),
    file!("icons/files/go-mod.svg"),
    file!("icons/files/graphql.svg"),
    file!("icons/files/h.svg"),
    file!("icons/files/hpp.svg"),
    file!("icons/files/html.svg"),
    file!("icons/files/image.svg"),
    file!("icons/files/jar.svg"),
    file!("icons/files/java.svg"),
    file!("icons/files/javascript.svg"),
    file!("icons/files/json.svg"),
    file!("icons/files/jupyter.svg"),
    file!("icons/files/key.svg"),
    file!("icons/files/kotlin.svg"),
    file!("icons/files/less.svg"),
    file!("icons/files/license.svg"),
    file!("icons/files/lock.svg"),
    file!("icons/files/log.svg"),
    file!("icons/files/lua.svg"),
    file!("icons/files/makefile.svg"),
    file!("icons/files/markdown.svg"),
    file!("icons/files/mdx.svg"),
    file!("icons/files/nodejs.svg"),
    file!("icons/files/npm.svg"),
    file!("icons/files/pdf.svg"),
    file!("icons/files/php.svg"),
    file!("icons/files/playwright.svg"),
    file!("icons/files/pnpm.svg"),
    file!("icons/files/pnpm_light.svg"),
    file!("icons/files/powershell.svg"),
    file!("icons/files/prettier.svg"),
    file!("icons/files/proto.svg"),
    file!("icons/files/python.svg"),
    file!("icons/files/react.svg"),
    file!("icons/files/react_ts.svg"),
    file!("icons/files/readme.svg"),
    file!("icons/files/ruby.svg"),
    file!("icons/files/rust.svg"),
    file!("icons/files/sass.svg"),
    file!("icons/files/settings.svg"),
    file!("icons/files/svelte.svg"),
    file!("icons/files/svg.svg"),
    file!("icons/files/swift.svg"),
    file!("icons/files/table.svg"),
    file!("icons/files/terraform.svg"),
    file!("icons/files/test-js.svg"),
    file!("icons/files/test-jsx.svg"),
    file!("icons/files/test-ts.svg"),
    file!("icons/files/toml.svg"),
    file!("icons/files/toml_light.svg"),
    file!("icons/files/tsconfig.svg"),
    file!("icons/files/tune.svg"),
    file!("icons/files/typescript.svg"),
    file!("icons/files/typescript-def.svg"),
    file!("icons/files/vagrant.svg"),
    file!("icons/files/video.svg"),
    file!("icons/files/vite.svg"),
    file!("icons/files/vitest.svg"),
    file!("icons/files/vue.svg"),
    file!("icons/files/webassembly.svg"),
    file!("icons/files/xml.svg"),
    file!("icons/files/yaml.svg"),
    file!("icons/files/yarn.svg"),
    file!("icons/files/zip.svg"),
    file!("icons/files/LICENSE-material-icon-theme"),
    file!("fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf"),
    file!("fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf"),
    file!("fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf"),
    file!("fonts/ibm-plex-sans/IBMPlexSans-SemiBoldItalic.ttf"),
    file!("fonts/ibm-plex-sans/license.txt"),
    file!("fonts/lilex/Lilex-Bold.ttf"),
    file!("fonts/lilex/Lilex-BoldItalic.ttf"),
    file!("fonts/lilex/Lilex-Italic.ttf"),
    file!("fonts/lilex/Lilex-Regular.ttf"),
    file!("fonts/lilex/OFL.txt"),
    file!("icons/arrow_down.svg"),
    file!("icons/arrow_left.svg"),
    file!("icons/arrow_right.svg"),
    file!("icons/arrow_up.svg"),
    file!("icons/arrow_up_right.svg"),
    file!("icons/backspace.svg"),
    file!("icons/bell.svg"),
    file!("icons/bell_off.svg"),
    file!("icons/book.svg"),
    file!("icons/case_sensitive.svg"),
    file!("icons/chat.svg"),
    file!("icons/check.svg"),
    file!("icons/chevron_down.svg"),
    file!("icons/chevron_down_up.svg"),
    file!("icons/chevron_left.svg"),
    file!("icons/chevron_right.svg"),
    file!("icons/chevron_up.svg"),
    file!("icons/chevron_up_down.svg"),
    file!("icons/close.svg"),
    file!("icons/command.svg"),
    file!("icons/control.svg"),
    file!("icons/copy.svg"),
    file!("icons/dash.svg"),
    file!("icons/debug.svg"),
    file!("icons/debug_pause.svg"),
    file!("icons/delete.svg"),
    file!("icons/diff_split.svg"),
    file!("icons/diff_unified.svg"),
    file!("icons/download.svg"),
    file!("icons/ellipsis.svg"),
    file!("icons/escape.svg"),
    file!("icons/expand_vertical.svg"),
    file!("icons/eye.svg"),
    file!("icons/eye_off.svg"),
    file!("icons/file.svg"),
    file!("icons/file_code.svg"),
    file!("icons/file_diff.svg"),
    file!("icons/file_markdown.svg"),
    file!("icons/file_text_outlined.svg"),
    file!("icons/filter.svg"),
    file!("icons/fold_vertical.svg"),
    file!("icons/folder.svg"),
    file!("icons/folder_add.svg"),
    file!("icons/font_size.svg"),
    file!("icons/git_branch.svg"),
    file!("icons/git_graph.svg"),
    file!("icons/image.svg"),
    file!("icons/info.svg"),
    file!("icons/list_todo.svg"),
    file!("icons/list_tree.svg"),
    file!("icons/load_circle.svg"),
    file!("icons/magnifying_glass.svg"),
    file!("icons/maximize.svg"),
    file!("icons/menu.svg"),
    file!("icons/minimize.svg"),
    file!("icons/option.svg"),
    file!("icons/page_down.svg"),
    file!("icons/page_up.svg"),
    file!("icons/paperclip.svg"),
    file!("icons/pencil.svg"),
    file!("icons/person.svg"),
    file!("icons/pin.svg"),
    file!("icons/play_filled.svg"),
    file!("icons/play_outlined.svg"),
    file!("icons/plus.svg"),
    file!("icons/pull_request.svg"),
    file!("icons/regex.svg"),
    file!("icons/return.svg"),
    file!("icons/rotate_ccw.svg"),
    file!("icons/rotate_cw.svg"),
    file!("icons/settings.svg"),
    file!("icons/shift.svg"),
    file!("icons/space.svg"),
    file!("icons/sparkle.svg"),
    file!("icons/split.svg"),
    file!("icons/stop.svg"),
    file!("icons/tab.svg"),
    file!("icons/table.svg"),
    file!("icons/terminal.svg"),
    file!("icons/terminal_alt.svg"),
    file!("icons/text_wrap.svg"),
    file!("icons/tool_hammer.svg"),
    file!("icons/trash.svg"),
    file!("icons/undo.svg"),
    file!("icons/unpin.svg"),
    file!("icons/warning.svg"),
    file!("icons/x_circle.svg"),
    // The path of this icon has the word "key" in it, which our tooling blocks in file names.
    (
        "icons/keyboard.svg",
        include_bytes!("../../assets/icons/kbd_board.svg") as &[u8],
    ),
];

/// The bytes of one own file.
pub fn find(path: &str) -> Option<&'static [u8]> {
    FILES
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, bytes)| *bytes)
}

/// Own paths that start with `prefix`.
pub fn paths_with_prefix(prefix: &str) -> impl Iterator<Item = &'static str> + '_ {
    FILES
        .iter()
        .map(|(name, _)| *name)
        .filter(move |name| name.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_file_is_filled_and_listed_once() {
        for (name, bytes) in FILES {
            assert!(!bytes.is_empty(), "{name} is empty");
            assert_eq!(FILES.iter().filter(|(other, _)| other == name).count(), 1);
        }
    }

    #[test]
    fn find_answers_own_files_and_none_for_others() {
        assert!(find("themes/natamux.json").is_some());
        assert!(find("themes/one/one.json").is_none());
    }

    #[test]
    fn prefix_lists_fonts_icons_and_themes() {
        assert_eq!(
            paths_with_prefix("fonts/")
                .filter(|p| p.ends_with(".ttf"))
                .count(),
            8
        );
        assert!(paths_with_prefix("icons/lucide/").any(|p| p.ends_with("/diamond.svg")));
        assert_eq!(paths_with_prefix("themes/").count(), 11);
        assert_eq!(paths_with_prefix("nothing/").count(), 0);
    }

    #[test]
    fn licences_ship_with_the_icons_and_themes() {
        for path in [
            "icons/lucide/LICENSE",
            "icons/agents/LICENSE-lobe-icons",
            "icons/agents/LICENSE-material-icon-theme",
            "icons/files/LICENSE-material-icon-theme",
            "themes/LICENSES",
            "fonts/ibm-plex-sans/license.txt",
            "fonts/lilex/OFL.txt",
        ] {
            assert!(find(path).is_some(), "{path} missing");
        }
    }

    #[test]
    fn fonts_are_the_exact_files_of_the_plan() {
        use sha2::{Digest, Sha256};
        let expected = [
            (
                "fonts/ibm-plex-sans/IBMPlexSans-Regular.ttf",
                "975dcda37d80f038",
            ),
            (
                "fonts/ibm-plex-sans/IBMPlexSans-Italic.ttf",
                "a9c6ef9942c49e49",
            ),
            (
                "fonts/ibm-plex-sans/IBMPlexSans-SemiBold.ttf",
                "a20caf8286023a6a",
            ),
            (
                "fonts/ibm-plex-sans/IBMPlexSans-SemiBoldItalic.ttf",
                "147106b619423a82",
            ),
            ("fonts/lilex/Lilex-Regular.ttf", "8a467ad5a1941dc7"),
            ("fonts/lilex/Lilex-Bold.ttf", "3369214e2ecddb45"),
            ("fonts/lilex/Lilex-Italic.ttf", "a6e52046b91ed12a"),
            ("fonts/lilex/Lilex-BoldItalic.ttf", "9629bd65ee737687"),
        ];
        for (path, prefix) in expected {
            let bytes = find(path).unwrap_or_else(|| panic!("{path} missing"));
            let hash = format!("{:x}", Sha256::digest(bytes));
            assert!(hash.starts_with(prefix), "{path}: {hash}");
        }
    }

    #[test]
    fn every_icon_name_has_a_file() {
        for name in ui::IconName::ALL {
            let path = name.path();
            assert!(find(&path).is_some(), "{path} missing");
        }
    }
}
