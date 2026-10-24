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
    file!("icons/motion/spin-dashes.svg"),
    file!("icons/motion/spin-dots.svg"),
    file!("icons/states/unknown-8.svg"),
    file!("icons/states/unknown.svg"),
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

    // The fonts are Zed's (its `assets` crate), so the app has none of its own.
    #[test]
    fn prefix_lists_icons_and_themes() {
        assert_eq!(paths_with_prefix("fonts/").count(), 0);
        assert!(paths_with_prefix("icons/lucide/").any(|p| p.ends_with("/diamond.svg")));
        assert_eq!(paths_with_prefix("themes/").count(), 1);
        assert_eq!(paths_with_prefix("nothing/").count(), 0);
    }

    #[test]
    fn licences_ship_with_the_icons() {
        for path in [
            "icons/lucide/LICENSE",
            "icons/agents/LICENSE-lobe-icons",
            "icons/agents/LICENSE-material-icon-theme",
        ] {
            assert!(find(path).is_some(), "{path} missing");
        }
    }
}
