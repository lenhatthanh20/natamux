//! Key bindings and menus, built from the command registry (`app/commands.rs`). Zed's default
//! keymap names actions of crates the app does not have, so the app binds only what its screens
//! use: the registry's commands, and the editor and terminal keys below.

use gpui::{Action, App, KeyBinding, Menu, MenuItem, NoAction, actions};

use super::commands::{COMMANDS, MenuName, command_by_id, keystroke, menu_items};
use super::commands::{is_single_key, reaches_app_from_terminal};
use crate::ime::ime_check::SubmitWord;

actions!(
    natamux,
    [
        Quit,
        Back,
        SubmitForm,
        ShowAllTasks,
        ShowTable,
        ShowBoard,
        ToggleSidebar,
        FocusFilter,
        NewWorkspace,
        NewTask,
        SelectNext,
        SelectPrevious,
        SelectFirst,
        SelectLast,
        OpenSelected,
        CollapseRow,
        ExpandRow,
        RenameRow,
        OpenRowMenu,
        RemoveRow,
        MarkReady,
        BlockTask,
        ResumeAgent,
        Approve,
        RequestChanges,
        UnblockTask,
        RetryTask,
        TogglePerfOverlay,
        SavePerfReport,
        StartBench,
        StopBench,
        CheckIme,
        CloseTool
    ]
);

/// Each registry id with its action. `binds_every_command_to_an_action` checks none is missing.
macro_rules! command_actions {
    ($($id:literal => $action:expr,)*) => {
        /// The action of a registry command, for menus and buttons.
        pub fn action_for(id: &str) -> Option<Box<dyn Action>> {
            match id {
                $($id => Some(Box::new($action)),)*
                _ => None,
            }
        }

        fn command_bindings(mac: bool) -> Vec<KeyBinding> {
            let mut bindings = Vec::new();
            $(push_bindings($id, $action, mac, &mut bindings);)*
            bindings
        }
    };
}

command_actions! {
    "app.quit" => Quit,
    "escape" => Back,
    "form.submit" => SubmitForm,
    "view.tasks" => ShowAllTasks,
    "view.table" => ShowTable,
    "view.board" => ShowBoard,
    "sidebar.toggle" => ToggleSidebar,
    "filter.focus" => FocusFilter,
    "workspace.new" => NewWorkspace,
    "task.new" => NewTask,
    "list.next" => SelectNext,
    "list.previous" => SelectPrevious,
    "list.first" => SelectFirst,
    "list.last" => SelectLast,
    "task.open" => OpenSelected,
    "tree.collapse" => CollapseRow,
    "tree.expand" => ExpandRow,
    "row.rename" => RenameRow,
    "row.menu" => OpenRowMenu,
    "row.remove" => RemoveRow,
    "task.task.readied" => MarkReady,
    "task.task.blocked" => BlockTask,
    "task.agent.resumed" => ResumeAgent,
    "task.review.approved" => Approve,
    "task.review.changes-requested" => RequestChanges,
    "task.task.unblocked" => UnblockTask,
    "task.task.retried" => RetryTask,
    "tools.perf-overlay" => TogglePerfOverlay,
    "tools.save-perf" => SavePerfReport,
    "tools.bench" => StartBench,
    "tools.stop-bench" => StopBench,
    "tools.ime-check" => CheckIme,
    "tools.close" => CloseTool,
    "tools.ime-submit" => SubmitWord,
}

fn push_bindings<A: Action + Clone>(id: &str, action: A, mac: bool, out: &mut Vec<KeyBinding>) {
    let Some(command) = command_by_id(id) else {
        return;
    };
    for key in command.keys {
        let stroke = keystroke(key, mac);
        if command.contexts.is_empty() {
            out.push(KeyBinding::new(&stroke, action.clone(), None));
        }
        for context in command.contexts {
            out.push(KeyBinding::new(&stroke, action.clone(), Some(context)));
        }
    }
}

/// Global chords the shell owns (Ctrl+B and the like off macOS) go to the terminal, not the app.
/// So does Esc: agents use it to stop (owner, 2026-10-22).
fn terminal_keeps(mac: bool) -> Vec<KeyBinding> {
    COMMANDS
        .iter()
        .filter(|command| command.contexts.is_empty())
        .flat_map(|command| command.keys.iter())
        .filter(|key| {
            (!is_single_key(key) || **key == "Escape") && !reaches_app_from_terminal(key, mac)
        })
        .map(|key| KeyBinding::new(&keystroke(key, mac), NoAction, Some("Terminal")))
        .collect()
}

/// cmd on macOS, ctrl elsewhere.
const MOD: &str = if cfg!(target_os = "macos") {
    "cmd"
} else {
    "ctrl"
};

pub fn bind(cx: &mut App) {
    cx.on_action(|_: &Quit, cx| cx.quit());
    let mac = cfg!(target_os = "macos");
    cx.bind_keys(command_bindings(mac));
    cx.bind_keys(terminal_keeps(mac));
    let m = |keys: &str| keys.replace("mod", MOD);
    let editor = Some("Editor");
    // A terminal sends ctrl-c and the like to the shell; only copy and paste are the app's.
    let (copy, paste) = if mac {
        ("cmd-c", "cmd-v")
    } else {
        ("ctrl-shift-c", "ctrl-shift-v")
    };
    cx.bind_keys([
        KeyBinding::new("backspace", editor::actions::Backspace, editor),
        KeyBinding::new("delete", editor::actions::Delete, editor),
        KeyBinding::new("left", editor::actions::MoveLeft, editor),
        KeyBinding::new("right", editor::actions::MoveRight, editor),
        KeyBinding::new("shift-left", editor::actions::SelectLeft, editor),
        KeyBinding::new("shift-right", editor::actions::SelectRight, editor),
        KeyBinding::new(
            "home",
            editor::actions::MoveToBeginningOfLine::default(),
            editor,
        ),
        KeyBinding::new("end", editor::actions::MoveToEndOfLine::default(), editor),
        KeyBinding::new(&m("mod-a"), editor::actions::SelectAll, editor),
        KeyBinding::new(&m("mod-c"), editor::actions::Copy, editor),
        KeyBinding::new(&m("mod-x"), editor::actions::Cut, editor),
        KeyBinding::new(&m("mod-v"), editor::actions::Paste, editor),
        KeyBinding::new(&m("mod-z"), editor::actions::Undo, editor),
        KeyBinding::new(&m("mod-shift-z"), editor::actions::Redo, editor),
        KeyBinding::new(copy, terminal::Copy, Some("Terminal")),
        KeyBinding::new(paste, terminal::Paste, Some("Terminal")),
    ]);
    if mac {
        cx.bind_keys([
            KeyBinding::new(
                "cmd-left",
                editor::actions::MoveToBeginningOfLine::default(),
                editor,
            ),
            KeyBinding::new(
                "cmd-right",
                editor::actions::MoveToEndOfLine::default(),
                editor,
            ),
            KeyBinding::new("alt-left", editor::actions::MoveToPreviousWordStart, editor),
            KeyBinding::new("alt-right", editor::actions::MoveToNextWordEnd, editor),
        ]);
    }
}

/// The menu bar (macOS): the registry's commands that have a menu, a separator between
/// sections. The menu shows each command's first key from the bindings.
pub fn set_menus(cx: &mut App) {
    cx.set_menus(MenuName::ALL.map(|name| Menu::new(name.title()).items(menu_of(name))));
}

fn menu_of(name: MenuName) -> Vec<MenuItem> {
    let mut items = Vec::new();
    let mut last_section = None;
    for (command, section) in menu_items(name) {
        let Some(action) = action_for(command.id) else {
            continue;
        };
        if last_section.is_some_and(|last| last != section) {
            items.push(MenuItem::separator());
        }
        last_section = Some(section);
        items.push(MenuItem::Action {
            name: command.title.into(),
            action,
            os_action: None,
            checked: false,
            disabled: false,
        });
    }
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_every_command_with_a_key_or_a_menu_bar_item_to_an_action() {
        let reached = COMMANDS
            .iter()
            .filter(|command| !command.keys.is_empty() || command.menu.is_some());
        for command in reached {
            assert!(action_for(command.id).is_some(), "{}", command.id);
        }
    }

    #[test]
    fn makes_one_binding_per_key_and_context() {
        let expected: usize = COMMANDS
            .iter()
            .map(|command| command.keys.len() * command.contexts.len().max(1))
            .sum();
        assert_eq!(command_bindings(true).len(), expected);
    }

    #[test]
    fn leaves_esc_and_the_shells_chords_to_the_terminal() {
        // On macOS only Esc: Cmd chords never reach a shell.
        assert_eq!(terminal_keeps(true).len(), 1);
        // Esc, and Mod+Q, Mod+B, Mod+[ and the Mod+Alt tools: Ctrl chords without Shift elsewhere.
        assert_eq!(terminal_keeps(false).len(), 10);
    }
}
