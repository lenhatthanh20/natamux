use std::collections::BTreeMap;

use natamux_proto::tasks::task::TaskState;

use super::*;

#[test]
fn gives_every_command_its_own_id() {
    let mut ids: Vec<&str> = all_commands().map(|command| command.id).collect();
    ids.sort_unstable();
    let count = ids.len();
    ids.dedup();
    assert_eq!(ids.len(), count);
}

/// Both platforms, as gpui gets them.
fn bindings(mac: bool) -> Vec<(&'static str, String, &'static str)> {
    let mut out = Vec::new();
    for command in all_commands().filter(|command| crate::app::shortcuts::is_bound(command)) {
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
        command_by_id("palette.open").map(|c| c.keys),
        Some(&["Mod+K", "F1"][..])
    );
    assert_eq!(shortcut_of("jump.open"), Some("Mod+J"));
    assert_eq!(shortcut_of("app.settings"), Some("Mod+,"));
    assert_eq!(shortcut_of("help.shortcuts"), Some("?"));
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
    for command in all_commands().filter(|command| command.menu.is_some()) {
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
    let agents = CommandState {
        agents_open: true,
        ..CommandState::default()
    };
    assert_eq!(next.map(|when| when.holds(&agents)), Some(false));
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

#[test]
fn has_the_keys_of_the_details_panel() {
    assert_eq!(shortcut_of("details.toggle"), Some("Mod+Alt+B"));
    assert_eq!(shortcut_of("task.details"), Some("i"));
    assert_eq!(shortcut_of("task.edit"), Some("e"));
    assert_eq!(shortcut_of("tools.bench"), Some("Mod+Alt+T"));
    let approve = command_by_id("task.review.approved").map(|command| command.contexts);
    assert!(approve.is_some_and(|contexts| contexts.contains(&DETAILS)));
}

#[test]
fn offers_the_stage_commands_only_in_an_open_workspace_with_a_stage_at_work() {
    let skip = command_by_id("stage.skip").map(|command| command.when);
    let state = |task_open: bool, selected_state: Option<TaskState>| CommandState {
        task_open,
        has_selection: selected_state.is_some(),
        selected_state,
        ..CommandState::default()
    };
    let holds = |state: CommandState| skip.is_some_and(|when| when.holds(&state));
    assert!(holds(state(true, Some(TaskState::Review))));
    assert!(holds(state(true, Some(TaskState::NeedsInput))));
    assert!(!holds(state(false, Some(TaskState::Review))));
    assert!(!holds(state(true, Some(TaskState::Done))));
}

#[test]
fn drafts_the_commit_message_with_mod_shift_g_only_from_the_commit_box() {
    let draft = command_by_id("diff.draft-commit");
    assert_eq!(shortcut_of("diff.draft-commit"), Some("Mod+Shift+G"));
    assert_eq!(
        draft.map(|command| command.contexts),
        Some(&[COMMIT_BOX][..])
    );
}

#[test]
fn offers_the_workspace_tabs_and_files_only_with_a_workspace_open() {
    let ids = [
        "tab.terminal",
        "tab.changes",
        "tab.history",
        "diff.next-file",
        "diff.prev-file",
        "changes.refresh",
        "task.commit",
        "tab.docs",
        "tab.editor",
        "files.show",
        "file.save",
        "tab.close",
        "tab.reopen",
    ];
    let open = CommandState {
        workspace_open: true,
        ..CommandState::default()
    };
    for id in ids {
        let when = command_by_id(id).map(|command| command.when);
        assert_eq!(when, Some(When::InWorkspace), "{id}");
        assert_eq!(
            when.map(|when| when.holds(&CommandState::default())),
            Some(false),
            "{id}"
        );
        assert_eq!(when.map(|when| when.holds(&open)), Some(true), "{id}");
    }
}

#[test]
fn has_the_keys_of_the_changes_tab() {
    let keys_of = |id| command_by_id(id).map(|command| command.keys);
    assert_eq!(keys_of("tab.terminal"), Some(&["1"][..]));
    assert_eq!(keys_of("tab.changes"), Some(&["2"][..]));
    assert_eq!(keys_of("tab.history"), Some(&["3"][..]));
    assert_eq!(keys_of("changes.refresh"), Some(&["Shift+R"][..]));
    assert_eq!(keys_of("diff.next-change"), Some(&["Alt+F5"][..]));
    assert_eq!(keys_of("diff.prev-change"), Some(&["Shift+Alt+F5"][..]));
    assert_eq!(keys_of("changes.rollback"), Some(&["Mod+Alt+Z"][..]));
    assert_eq!(shortcut_of("task.commit"), None);
}

#[test]
fn keeps_single_keys_out_of_text_boxes_and_terminals() {
    for command in all_commands().filter(|command| command.keys.iter().any(|k| is_single_key(k))) {
        assert!(!command.contexts.contains(&TERMINAL), "{}", command.id);
        assert!(!command.contexts.contains(&CHANGES_ANY), "{}", command.id);
        assert!(!command.contexts.contains(&COMMIT_BOX), "{}", command.id);
    }
}

#[test]
fn binds_no_command_to_the_5_key_which_showed_the_docs_tab_before() {
    assert_eq!(shortcut_of("tab.docs"), Some("4"));
    for command in all_commands() {
        let four = command.keys.contains(&"4") && command.id != "tab.docs";
        assert!(!four && !command.keys.contains(&"5"), "{}", command.id);
    }
    assert_eq!(shortcut_of("tab.editor"), None);
    assert_eq!(shortcut_of("file.save"), Some("Mod+S"));
}

#[test]
fn offers_the_stage_actions_in_an_open_workspace_of_a_task_that_is_not_finished() {
    let state = |selected_state| CommandState {
        task_open: true,
        workspace_open: true,
        has_selection: true,
        selected_state: Some(selected_state),
        ..CommandState::default()
    };
    for id in [
        "stage.skip",
        "stage.report",
        "stage.approve",
        "stage.send-back",
    ] {
        let command = command_by_id(id);
        assert_eq!(
            command.map(|c| (c.hidden, c.category)),
            Some((false, Category::Workspace))
        );
        let when = command.map(|c| c.when);
        assert_eq!(
            when.map(|w| w.holds(&state(TaskState::Running))),
            Some(true),
            "{id}"
        );
        assert_eq!(
            when.map(|w| w.holds(&state(TaskState::Done))),
            Some(false),
            "{id}"
        );
        assert_eq!(
            when.map(|w| w.holds(&CommandState::default())),
            Some(false),
            "{id}"
        );
    }
}

#[test]
fn does_not_run_entries_that_only_document_a_key() {
    for id in [
        "list.ends",
        "tree.keys",
        "stage.move",
        "palette.move",
        "palette.open-row",
        "palette.digit",
        "palette.scope",
    ] {
        let command = command_by_id(id);
        assert!(
            command.is_some_and(|c| c.hidden && c.group.is_some() && c.documents_only),
            "{id}"
        );
        assert!(crate::app::shortcuts::action_for(id).is_none(), "{id}");
    }
}

#[test]
fn goes_through_the_actions_of_the_app() {
    let name = |id: &str| crate::app::shortcuts::action_for(id).map(|action| action.name());
    assert_eq!(name("palette.open"), Some("natamux::OpenPalette"));
    assert_eq!(name("jump.open"), Some("natamux::OpenJump"));
    assert_eq!(name("help.shortcuts"), Some("natamux::ShowShortcuts"));
    assert_eq!(name("app.settings"), Some("natamux::OpenSettings"));
    assert_eq!(name("repo.add"), Some("natamux::AddProject"));
    assert_eq!(name("repo.import"), Some("natamux::ImportIssues"));
}

#[test]
fn writes_the_palette_title_with_its_category() {
    let title = |id: &str| command_by_id(id).map(super::super::command_types::palette_title);
    assert_eq!(
        title("task.review.approved").as_deref(),
        Some("Task: Approve")
    );
    assert_eq!(title("repo.add").as_deref(), Some("Project: Add a project"));
    assert_eq!(
        title("workspace.new").as_deref(),
        Some("Task: Start a task")
    );
}

/// Review 1: the action check of `shortcuts.rs` failed on `list.ends`, which only names keys.
#[test]
fn only_the_sheet_entries_have_keys_and_no_action() {
    let without: Vec<&str> = all_commands()
        .filter(|command| !command.keys.is_empty())
        .filter(|command| crate::app::shortcuts::action_for(command.id).is_none())
        .map(|command| command.id)
        .collect();
    let documented: Vec<&str> = all_commands()
        .filter(|command| command.documents_only)
        .map(|command| command.id)
        .collect();
    assert_eq!(without, documented);
}

#[test]
fn suggests_the_commands_of_the_web_in_its_order() {
    let mut suggested: Vec<(u8, &str)> = all_commands()
        .filter_map(|command| command.suggested.map(|rank| (rank, command.id)))
        .collect();
    suggested.sort();
    let ids: Vec<&str> = suggested.iter().map(|(_, id)| *id).collect();
    assert_eq!(
        ids,
        [
            "jump.open",
            "help.shortcuts",
            "workspace.new",
            "task.new",
            "view.agents"
        ]
    );
}

/// T-0158: the tabs close as Zed's pane tabs, and the status bar toggles the Agents grid.
#[test]
fn closes_and_reopens_tabs_and_toggles_the_agents() {
    let name = |id: &str| crate::app::shortcuts::action_for(id).map(|action| action.name());
    assert_eq!(shortcut_of("tab.close"), Some("Mod+W"));
    assert_eq!(shortcut_of("tab.reopen"), None);
    assert_eq!(shortcut_of("agents.toggle"), None);
    assert_eq!(name("tab.close"), Some("natamux::CloseTab"));
    assert_eq!(name("tab.reopen"), Some("natamux::ReopenTabs"));
    assert_eq!(name("agents.toggle"), Some("natamux::ToggleAgents"));
}
