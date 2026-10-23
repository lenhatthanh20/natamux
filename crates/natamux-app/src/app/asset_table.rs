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
    file!("ime-words.txt"),
    file!("icons/lucide/arrow-down.svg"),
    file!("icons/lucide/arrow-left.svg"),
    file!("icons/lucide/arrow-up-from-line.svg"),
    file!("icons/lucide/arrow-up.svg"),
    file!("icons/lucide/ban.svg"),
    file!("icons/lucide/bell-off.svg"),
    file!("icons/lucide/bell.svg"),
    file!("icons/states/blocked-8.svg"),
    file!("icons/states/blocked.svg"),
    file!("icons/lucide/bot.svg"),
    file!("icons/states/cancelled-8.svg"),
    file!("icons/states/cancelled.svg"),
    file!("icons/lucide/case-sensitive.svg"),
    file!("icons/lucide/check.svg"),
    file!("icons/lucide/chevron-down.svg"),
    file!("icons/lucide/chevron-left.svg"),
    file!("icons/lucide/chevron-right.svg"),
    file!("icons/lucide/chevron-up.svg"),
    file!("icons/lucide/circle-alert.svg"),
    file!("icons/lucide/circle-check.svg"),
    file!("icons/lucide/circle-dot.svg"),
    file!("icons/lucide/circle-pause.svg"),
    file!("icons/lucide/circle-play.svg"),
    file!("icons/lucide/circle-user.svg"),
    file!("icons/agents/claude.svg"),
    file!("icons/agents/codex.svg"),
    file!("icons/lucide/columns-3.svg"),
    file!("icons/agents/copilot.svg"),
    file!("icons/agents/copilot_light.svg"),
    file!("icons/lucide/copy-plus.svg"),
    file!("icons/lucide/copy.svg"),
    file!("icons/agents/cursor.svg"),
    file!("icons/agents/cursor_light.svg"),
    file!("icons/states/done-8.svg"),
    file!("icons/states/done.svg"),
    file!("icons/lucide/diamond.svg"),
    file!("icons/lucide/download.svg"),
    file!("icons/states/draft-8.svg"),
    file!("icons/states/draft.svg"),
    file!("icons/lucide/ellipsis.svg"),
    file!("icons/lucide/external-link.svg"),
    file!("icons/lucide/eye-off.svg"),
    file!("icons/lucide/eye.svg"),
    file!("icons/states/failed-8.svg"),
    file!("icons/states/failed.svg"),
    file!("icons/lucide/file-diff.svg"),
    file!("icons/lucide/file-text.svg"),
    file!("icons/lucide/folder-git-2.svg"),
    file!("icons/lucide/folder-minus.svg"),
    file!("icons/lucide/folder-plus.svg"),
    file!("icons/agents/gemini-ai.svg"),
    file!("icons/lucide/git-branch.svg"),
    file!("icons/lucide/git-pull-request.svg"),
    file!("icons/lucide/info.svg"),
    file!("fonts/inter/Inter-Bold.ttf"),
    file!("fonts/inter/Inter-Medium.ttf"),
    file!("fonts/inter/Inter-Regular.ttf"),
    file!("fonts/inter/Inter-SemiBold.ttf"),
    file!("fonts/jetbrains-mono/JetBrainsMono-Bold.ttf"),
    file!("fonts/jetbrains-mono/JetBrainsMono-Italic.ttf"),
    file!("fonts/jetbrains-mono/JetBrainsMono-Regular.ttf"),
    file!("icons/lucide/keyboard.svg"),
    file!("icons/lucide/layout-grid.svg"),
    file!("icons/lucide/LICENSE"),
    file!("icons/agents/LICENSE-lobe-icons"),
    file!("icons/agents/LICENSE-material-icon-theme"),
    file!("icons/lucide/list-checks.svg"),
    file!("icons/lucide/list-filter.svg"),
    file!("icons/lucide/list-plus.svg"),
    file!("icons/lucide/list-todo.svg"),
    file!("icons/lucide/list-tree.svg"),
    file!("icons/lucide/loader-circle.svg"),
    file!("icons/lucide/loader.svg"),
    file!("icons/lucide/maximize-2.svg"),
    file!("icons/lucide/menu.svg"),
    file!("icons/lucide/minimize-2.svg"),
    file!("icons/lucide/minus.svg"),
    file!("icons/states/needs-input-8.svg"),
    file!("icons/states/needs-input.svg"),
    file!("fonts/inter/OFL.txt"),
    file!("fonts/jetbrains-mono/OFL.txt"),
    file!("icons/lucide/panel-left.svg"),
    file!("icons/lucide/panel-right.svg"),
    file!("icons/lucide/paperclip.svg"),
    file!("icons/lucide/pencil.svg"),
    file!("icons/lucide/pin-off.svg"),
    file!("icons/lucide/pin.svg"),
    file!("icons/lucide/play.svg"),
    file!("icons/lucide/plus.svg"),
    file!("icons/states/quiet-8.svg"),
    file!("icons/states/quiet.svg"),
    file!("icons/states/ready-8.svg"),
    file!("icons/states/ready.svg"),
    file!("icons/lucide/regex.svg"),
    file!("icons/states/review-8.svg"),
    file!("icons/states/review.svg"),
    file!("icons/lucide/rotate-ccw.svg"),
    file!("icons/lucide/rows-3.svg"),
    file!("icons/states/running-8.svg"),
    file!("icons/states/running.svg"),
    file!("icons/lucide/save.svg"),
    file!("icons/lucide/search-x.svg"),
    file!("icons/lucide/search.svg"),
    file!("icons/lucide/settings.svg"),
    file!("icons/lucide/sparkles.svg"),
    file!("icons/lucide/square-split-horizontal.svg"),
    file!("icons/lucide/square-terminal.svg"),
    file!("icons/lucide/terminal.svg"),
    file!("icons/lucide/trash.svg"),
    file!("icons/lucide/triangle-alert.svg"),
    file!("icons/lucide/undo-2.svg"),
    file!("icons/states/unknown-8.svg"),
    file!("icons/states/unknown.svg"),
    file!("icons/lucide/x.svg"),
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
        let fonts = paths_with_prefix("fonts/").filter(|p| p.ends_with(".ttf"));
        assert_eq!(fonts.count(), 7);
        assert!(paths_with_prefix("icons/lucide/").any(|p| p.ends_with("/check.svg")));
        assert_eq!(paths_with_prefix("themes/").count(), 1);
        assert_eq!(paths_with_prefix("nothing/").count(), 0);
    }

    #[test]
    fn licences_ship_with_fonts_and_icons() {
        for path in [
            "fonts/inter/OFL.txt",
            "fonts/jetbrains-mono/OFL.txt",
            "icons/lucide/LICENSE",
            "icons/agents/LICENSE-lobe-icons",
            "icons/agents/LICENSE-material-icon-theme",
        ] {
            assert!(find(path).is_some(), "{path} missing");
        }
    }
}
