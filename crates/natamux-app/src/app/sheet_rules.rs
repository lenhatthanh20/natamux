//! The rows of the shortcut sheet (`sheetGroups` of `lib/commands` and `matches` of
//! `ShortcutSheet.tsx`): every command of a sheet group, in the groups of section 5.1.

use super::command_types::{Command, SheetGroup};
use super::shortcut_text::key_text;

#[derive(Clone, Debug, PartialEq)]
pub struct SheetSection {
    pub group: SheetGroup,
    pub commands: Vec<&'static Command>,
}

/// The groups in the sheet's order, each with its commands in registry order. Empty groups go.
pub fn sheet_groups(commands: &[&'static Command]) -> Vec<SheetSection> {
    SheetGroup::ALL
        .into_iter()
        .map(|group| SheetSection {
            group,
            commands: commands
                .iter()
                .copied()
                .filter(|command| command.group == Some(group))
                .collect(),
        })
        .filter(|section| !section.commands.is_empty())
        .collect()
}

/// Every word of the query is in the title or in the text of one of the keys, ignoring case.
pub fn sheet_matches(command: &Command, query: &str, mac: bool) -> bool {
    let parts: Vec<String> = std::iter::once(command.title.to_lowercase())
        .chain(
            command
                .keys
                .iter()
                .map(|key| key_text(key, mac).to_lowercase()),
        )
        .collect();
    query
        .to_lowercase()
        .split_whitespace()
        .all(|word| parts.iter().any(|part| part.contains(word)))
}

/// The sheet after the search field: matching commands only, empty groups gone.
pub fn filtered(sections: &[SheetSection], query: &str, mac: bool) -> Vec<SheetSection> {
    sections
        .iter()
        .map(|section| SheetSection {
            group: section.group,
            commands: section
                .commands
                .iter()
                .copied()
                .filter(|command| sheet_matches(command, query, mac))
                .collect(),
        })
        .filter(|section| !section.commands.is_empty())
        .collect()
}

/// "No shortcut matches “<query>”."
pub fn no_match_text(query: &str) -> String {
    format!("No shortcut matches \u{201c}{}\u{201d}.", query.trim())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::commands::{all_commands, command_by_id};

    #[test]
    fn lists_every_group_of_the_keyboard_map_in_the_shortcut_sheet_each_with_keys() {
        let all: Vec<_> = all_commands().collect();
        let groups: Vec<SheetGroup> = sheet_groups(&all)
            .into_iter()
            .map(|section| section.group)
            .collect();
        assert_eq!(groups, SheetGroup::ALL);
        for command in all_commands().filter(|command| command.group.is_some()) {
            assert!(!command.keys.is_empty(), "{}", command.id);
        }
    }

    #[test]
    fn finds_a_shortcut_by_its_title_or_its_key() {
        let Some(palette) = command_by_id("palette.open") else {
            panic!("no palette command");
        };
        assert!(sheet_matches(palette, "command PALETTE", true));
        assert!(sheet_matches(palette, "⌘k", true));
        assert!(sheet_matches(palette, "ctrl+k", false));
        assert!(!sheet_matches(palette, "palette sidebar", true));
        assert!(sheet_matches(palette, "  ", true));
    }

    #[test]
    fn drops_the_groups_with_no_match() {
        let all: Vec<_> = all_commands().collect();
        let sections = sheet_groups(&all);
        let found = filtered(&sections, "terminal", true);
        assert!(
            found
                .iter()
                .any(|section| section.group == SheetGroup::Panes)
        );
        assert!(found.iter().all(|section| !section.commands.is_empty()));
        assert_eq!(filtered(&sections, "zzz", true), Vec::new());
        assert_eq!(
            no_match_text(" zzz "),
            "No shortcut matches \u{201c}zzz\u{201d}."
        );
    }
}
