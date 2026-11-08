//! T-0316: the editor keys and the find, go to line and symbol commands.

use super::super::commands::{EDITOR_COMMANDS, TERMINAL, all_commands, command_by_id, keystroke};

/// T-0316: an editor key never hides a key that works everywhere, and the stage list keeps
/// Alt+Up and Alt+Down to itself.
#[test]
fn editor_keys_do_not_take_a_global_key() {
    for mac in [true, false] {
        let global: Vec<String> = all_commands()
            .filter(|command| command.contexts.is_empty())
            .flat_map(|command| command.keys.iter().map(move |key| keystroke(key, mac)))
            .collect();
        for command in EDITOR_COMMANDS {
            assert!(!command.contexts.is_empty(), "{}", command.id);
            for key in command.keys {
                let stroke = keystroke(key, mac);
                assert!(!global.contains(&stroke), "{} {stroke}", command.id);
                assert!(!stroke.starts_with("cmd-k") && !stroke.starts_with("ctrl-k"));
            }
        }
    }
    for id in ["stage.move-up", "stage.move-down"] {
        let contexts = command_by_id(id).map(|command| command.contexts);
        assert_eq!(
            contexts,
            Some(&[crate::app::commands_stages::STAGE_LIST][..]),
            "{id}"
        );
    }
}

#[test]
fn opens_find_go_to_line_and_symbols_in_the_diff_and_the_editor_tab() {
    use crate::app::commands_editor::{DIFF_VIEW, FILE_VIEW};
    for (id, key) in [
        ("editor.find", "Mod+F"),
        ("editor.go-to-line", "Ctrl+G"),
        ("editor.outline", "Mod+Shift+O"),
    ] {
        let command = command_by_id(id);
        assert_eq!(command.map(|c| c.keys), Some(&[key][..]), "{id}");
        assert_eq!(
            command.map(|c| c.contexts),
            Some(&[DIFF_VIEW, FILE_VIEW][..])
        );
        assert!(crate::app::shortcuts::action_for(id).is_some(), "{id}");
    }
    // The terminal keeps its own find.
    assert_eq!(
        command_by_id("terminal.find").map(|c| c.contexts),
        Some(&[TERMINAL][..])
    );
}
