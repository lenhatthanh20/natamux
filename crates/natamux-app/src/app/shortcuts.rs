//! Key bindings and menus, built from the command registry (`app/commands.rs`). Zed's default
//! keymap names actions of crates the app does not have, so the app binds only what its screens
//! use: the registry's commands, and the editor and terminal keys below.

use std::rc::Rc;

use gpui::{
    Action, App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate, Menu, MenuItem,
    NoAction, actions,
};

use super::commands::{Command, is_single_key, reaches_app_from_terminal};
use super::commands::{MenuName, all_commands, command_by_id, keystroke, menu_items};
pub use super::commands_app::{
    AddProject, ImportIssues, OpenJump, OpenPalette, OpenSettings, ShowShortcuts, UseCompactSize,
    UseDarkTheme, UseDefaultSize, UseLargeSize, UseLightTheme, UseSystemTheme,
};
pub use super::commands_files::{
    CloseTab, ReopenTabs, SaveFile, ShowDocsTab, ShowOpenFile, ShowWorktreeFiles,
};
pub use super::commands_stages::{MoveStageDown, MoveStageUp, ShowTasksProgress};
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
        CloseTool,
        ShowAgents,
        ToggleAgents,
        MaximizePane,
        NewTerminal,
        FindInTerminal,
        FindNext,
        FindPrevious,
        ToggleDetails,
        ToggleSelectedDetails,
        EditTask,
        ShowProgress,
        ApproveStage,
        ReportStage,
        SkipStage,
        SendBackStage,
        ShowTerminalTab,
        ShowChangesTab,
        ShowHistoryTab,
        RefreshChanges,
        PrevFile,
        NextFile,
        NextChange,
        PrevChange,
        CommentOnDiff,
        RollbackFile,
        DraftCommit,
        CommitChanges
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
    "view.agents" => ShowAgents,
    "agents.toggle" => ToggleAgents,
    "pane.maximize" => MaximizePane,
    "shell.new" => NewTerminal,
    "terminal.find" => FindInTerminal,
    "find.next" => FindNext,
    "find.previous" => FindPrevious,
    "details.toggle" => ToggleDetails,
    "task.details" => ToggleSelectedDetails,
    "task.edit" => EditTask,
    "progress.show" => ShowProgress,
    "stage.approve" => ApproveStage,
    "stage.report" => ReportStage,
    "stage.skip" => SkipStage,
    "stage.send-back" => SendBackStage,
    "tab.terminal" => ShowTerminalTab,
    "tab.changes" => ShowChangesTab,
    "tab.history" => ShowHistoryTab,
    "changes.refresh" => RefreshChanges,
    "diff.prev-file" => PrevFile,
    "diff.next-file" => NextFile,
    "diff.next-change" => NextChange,
    "diff.prev-change" => PrevChange,
    "diff.comment" => CommentOnDiff,
    "changes.rollback" => RollbackFile,
    "diff.draft-commit" => DraftCommit,
    "task.commit" => CommitChanges,
    "view.progress" => ShowTasksProgress,
    "stage.move-up" => MoveStageUp,
    "stage.move-down" => MoveStageDown,
    "tab.docs" => ShowDocsTab,
    "tab.editor" => ShowOpenFile,
    "files.show" => ShowWorktreeFiles,
    "file.save" => SaveFile,
    "tab.close" => CloseTab,
    "tab.reopen" => ReopenTabs,
    "palette.open" => OpenPalette,
    "jump.open" => OpenJump,
    "help.shortcuts" => ShowShortcuts,
    "app.settings" => OpenSettings,
    "theme.system" => UseSystemTheme,
    "theme.light" => UseLightTheme,
    "theme.dark" => UseDarkTheme,
    "ui-size.compact" => UseCompactSize,
    "ui-size.default" => UseDefaultSize,
    "ui-size.large" => UseLargeSize,
    "repo.add" => AddProject,
    "repo.import" => ImportIssues,
}

/// A command with an action. The others only name keys in the shortcut sheet, or are menu items
/// that the row and project menus run.
pub fn is_bound(command: &Command) -> bool {
    action_for(command.id).is_some()
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
    all_commands()
        .filter(|command| command.contexts.is_empty() && is_bound(command))
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
    bind_full_editor(cx, mac);
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

/// A full editor (the editor tab, the Docs edit) edits many lines. Single-line boxes leave the
/// arrows and Enter to the lists and forms around them.
fn bind_full_editor(cx: &mut App, mac: bool) {
    let full = "Editor && mode == full";
    let context = Some(full);
    let (start, end) = if mac {
        ("cmd-up", "cmd-down")
    } else {
        ("ctrl-home", "ctrl-end")
    };
    cx.bind_keys([
        KeyBinding::new("enter", editor::actions::Newline, context),
        KeyBinding::new("tab", editor::actions::Tab, context),
        KeyBinding::new("shift-tab", editor::actions::Backtab, context),
        KeyBinding::new("shift-up", editor::actions::SelectUp, context),
        KeyBinding::new("shift-down", editor::actions::SelectDown, context),
        KeyBinding::new("pageup", editor::actions::MovePageUp::default(), context),
        KeyBinding::new(
            "pagedown",
            editor::actions::MovePageDown::default(),
            context,
        ),
        KeyBinding::new(start, editor::actions::MoveToBeginning, context),
        KeyBinding::new(end, editor::actions::MoveToEnd, context),
    ]);
    // Zed's MoveUp and MoveDown live in a crate the app does not take: they bind by name.
    let by_name = [("up", "editor::MoveUp"), ("down", "editor::MoveDown")]
        .into_iter()
        .filter_map(|(keys, name)| {
            let action = cx.build_action(name, None).ok()?;
            let predicate = KeyBindingContextPredicate::parse(full).ok()?;
            KeyBinding::load(
                keys,
                action,
                Some(Rc::new(predicate)),
                false,
                None,
                &DummyKeyboardMapper,
            )
            .ok()
        })
        .collect::<Vec<_>>();
    cx.bind_keys(by_name);
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
            name: menu_title(command).into(),
            action,
            os_action: None,
            checked: false,
            disabled: false,
        });
    }
    items
}

/// A menu item's text where it differs from the command's title.
fn menu_title(command: &Command) -> &'static str {
    match command.id {
        "app.settings" => "Settings\u{2026}",
        _ => command.title,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn binds_every_command_with_a_key_or_a_menu_bar_item_to_an_action() {
        let reached = all_commands().filter(|command| {
            !command.documents_only && (!command.keys.is_empty() || command.menu.is_some())
        });
        for command in reached {
            assert!(action_for(command.id).is_some(), "{}", command.id);
        }
    }

    #[test]
    fn makes_one_binding_per_key_and_context() {
        let expected: usize = all_commands()
            .filter(|command| is_bound(command))
            .map(|command| command.keys.len() * command.contexts.len().max(1))
            .sum();
        assert_eq!(command_bindings(true).len(), expected);
    }

    #[test]
    fn leaves_esc_and_the_shells_chords_to_the_terminal() {
        // On macOS only Esc: Cmd chords never reach a shell.
        assert_eq!(terminal_keeps(true).len(), 1);
        // Esc, and Mod+Q, Mod+B, Mod+[, Mod+Alt+B, Mod+K, Mod+,, Mod+J, Mod+W (a shell's delete
        // word) and the Mod+Alt tools: Ctrl chords without Shift elsewhere. F1 reaches the app.
        assert_eq!(terminal_keeps(false).len(), 15);
    }
}
