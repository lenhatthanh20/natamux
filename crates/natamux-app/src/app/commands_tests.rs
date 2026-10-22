use std::collections::BTreeMap;

use super::*;

#[test]
fn gives_every_command_its_own_id() {
    let mut ids: Vec<&str> = COMMANDS.iter().map(|command| command.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count);
}

/// Both platforms, as gpui gets them.
fn bindings(mac: bool) -> Vec<(&'static str, String, &'static str)> {
    let mut out = Vec::new();
    for command in COMMANDS {
        for key in command.keys {
            let stroke = keystroke(key, mac);
            if command.contexts.is_empty() {
                out.push(("", stroke.clone(), command.id));
            }
            for context in command.contexts {
                out.push((*context, stroke.clone(), command.id));
            }
        }
    }
    out
}

fn duplicates(bindings: &[(&str, String, &str)]) -> Vec<String> {
    let mut seen: BTreeMap<(&str, &str), &str> = BTreeMap::new();
    let mut global: BTreeMap<&str, &str> = BTreeMap::new();
    for (context, stroke, id) in bindings {
        if context.is_empty() {
            global.insert(stroke, id);
        }
    }
    let mut found = Vec::new();
    for (context, stroke, id) in bindings {
        if let Some(other) = seen.insert((context, stroke), id) {
            found.push(format!("{stroke} in {context:?}: {other} and {id}"));
        }
        // A global key under a context key would be shadowed there.
        if let Some(other) = global.get(stroke.as_str())
            && !context.is_empty()
        {
            found.push(format!("{stroke}: global {other} and {id} in {context}"));
        }
    }
    found
}

#[test]
fn binds_a_key_to_one_command_only_in_each_context() {
    for mac in [true, false] {
        assert_eq!(
            duplicates(&bindings(mac)),
            Vec::<String>::new(),
            "mac: {mac}"
        );
    }
}

#[test]
fn the_duplicate_check_fails_for_two_commands_with_one_key_in_one_context() {
    let mut bad = bindings(true);
    bad.push((LIST, "c".into(), "task.extra"));
    bad.push(("Other", "cmd-q".into(), "task.shadowed"));
    assert_eq!(
        duplicates(&bad),
        [
            "c in \"NatamuxList && not_editing\": workspace.new and task.extra",
            "cmd-q: global app.quit and task.shadowed in Other",
        ]
    );
}

#[test]
fn has_the_keys_of_section_5_1_of_the_ui_spec() {
    assert_eq!(shortcut_of("sidebar.toggle"), Some("Mod+B"));
    assert_eq!(shortcut_of("filter.focus"), Some("/"));
    assert_eq!(shortcut_of("workspace.new"), Some("c"));
    assert_eq!(shortcut_of("task.new"), Some("n"));
    assert_eq!(shortcut_of("row.rename"), Some("F2"));
    assert_eq!(shortcut_of("row.menu"), Some("Shift+F10"));
    assert_eq!(shortcut_of("form.submit"), Some("Mod+Enter"));
    assert_eq!(
        command_by_id("list.next").map(|c| c.keys),
        Some(&["j", "ArrowDown"][..])
    );
}

#[test]
fn keeps_cancel_task_out_of_the_keys_and_the_palette_because_it_asks_first() {
    assert!(command_by_id("task.task.cancelled").is_none());
}

// A menu's key works with no modifier on macOS: a single key there would fire while the
// user types in a text box.
#[test]
fn puts_no_single_key_command_in_the_menu_bar() {
    for command in COMMANDS.iter().filter(|command| command.menu.is_some()) {
        assert!(
            command.keys.iter().all(|key| !is_single_key(key)),
            "{}",
            command.id
        );
    }
}

#[test]
fn lists_the_tools_of_phase_1_in_the_tools_menu() {
    let tools: Vec<(&str, u8)> = menu_items(MenuName::Tools)
        .map(|(command, section)| (command.title, section))
        .collect();
    assert_eq!(
        tools,
        [
            ("Toggle Perf Overlay", 0),
            ("Save Perf Report", 0),
            ("Bench: 10 Terminals", 1),
            ("Stop Bench", 1),
            ("IME Check", 2),
            ("Back to the Terminal", 2),
        ]
    );
}

#[test]
fn moves_the_selection_in_the_task_list_only() {
    let next = command_by_id("list.next").map(|command| command.when);
    assert_eq!(
        next.map(|when| when.holds(&CommandState::default())),
        Some(true)
    );
    let open = CommandState {
        task_open: true,
        ..CommandState::default()
    };
    assert_eq!(next.map(|when| when.holds(&open)), Some(false));
}

#[test]
fn offers_approve_and_request_changes_in_the_review_state_only() {
    let approve = command_by_id("task.review.approved").map(|command| command.when);
    let state = |selected_state: Option<TaskState>| CommandState {
        has_selection: selected_state.is_some(),
        selected_state,
        ..CommandState::default()
    };
    assert_eq!(shortcut_of("task.review.approved"), Some("a"));
    assert_eq!(shortcut_of("task.review.changes-requested"), Some("r"));
    assert_eq!(
        approve.map(|when| when.holds(&state(Some(TaskState::Running)))),
        Some(false)
    );
    assert_eq!(approve.map(|when| when.holds(&state(None))), Some(false));
    assert_eq!(
        approve.map(|when| when.holds(&state(Some(TaskState::Review)))),
        Some(true)
    );
}

#[test]
fn writes_keys_as_gpui_keystrokes_per_platform() {
    assert_eq!(keystroke("Mod+Shift+Enter", true), "cmd-shift-enter");
    assert_eq!(keystroke("Mod+Shift+Enter", false), "ctrl-shift-enter");
    assert_eq!(keystroke("Shift+F10", true), "shift-f10");
    assert_eq!(keystroke("ContextMenu", false), "menu");
    assert_eq!(keystroke("ArrowDown", false), "down");
    assert_eq!(keystroke("Ctrl+Shift+`", true), "ctrl-shift-`");
    assert_eq!(keystroke("/", true), "/");
    assert_eq!(keystroke("Mod++", true), "cmd-+");
}

#[test]
fn writes_key_text_with_apple_signs_on_macos_only() {
    assert_eq!(key_text("Mod+Shift+Enter", true), "⇧⌘↵");
    assert_eq!(key_text("Mod+Shift+Enter", false), "Ctrl+Shift+Enter");
    assert_eq!(key_text("Mod+Alt+P", true), "⌥⌘P");
    assert_eq!(key_text("Escape", true), "Esc");
    assert_eq!(key_text("ArrowUp", false), "↑");
    assert_eq!(key_text("c", false), "C");
    assert_eq!(key_text("Shift+F10", false), "Shift+F10");
}

#[test]
fn leaves_keys_to_the_shell_except_cmd_function_and_ctrl_shift_chords() {
    assert!(reaches_app_from_terminal("Mod+B", true));
    assert!(!reaches_app_from_terminal("Mod+B", false));
    assert!(reaches_app_from_terminal("F2", false));
    assert!(reaches_app_from_terminal("Ctrl+Shift+`", false));
    assert!(!reaches_app_from_terminal("Escape", true));
    assert!(is_single_key("c"));
    assert!(is_single_key("Shift+R"));
    assert!(!is_single_key("F2"));
    assert!(!is_single_key("Mod+B"));
}
