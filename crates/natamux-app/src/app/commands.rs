//! The command registry (`packages/web/src/lib/commands`): each command's id, title, keys, where
//! the keys work and when it applies. Key bindings, the menu bar and the key text of menus and
//! tooltips all read it, so a key is defined once. `app/shortcuts.rs` gives each id its action.

use natamux_proto::tasks::task::TaskState;

use crate::tasks::task_rules::actions_for;

/// Key contexts (gpui predicates). Views add these names to their `KeyContext`.
/// Root adds `body` while its own focus handle, not a child's, has the focus: the web's "body".
pub const BODY: &str = "Natamux && body";
/// The sidebar tree, the task table and the board add `NatamuxList`, plus `not_editing` while no
/// inline rename is open, so single keys never reach a text box inside them.
pub const LIST: &str = "NatamuxList && not_editing";
pub const TREE: &str = "WorkspaceTree && not_editing";
pub const DIALOG: &str = "Dialog";
pub const IME_CHECK: &str = "ImeCheck";
/// Zed's terminal view, and the find bar of a pane (`terminals/terminal_find.rs`).
pub const TERMINAL: &str = "Terminal";
pub const TERMINAL_FIND: &str = "TerminalFind";
/// Where single keys work: outside text boxes and terminals.
const SINGLE: &[&str] = &[BODY, LIST];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Category {
    App,
    View,
    Task,
    Tools,
}

/// The menus of the menu bar, in their order.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuName {
    App,
    View,
    Tools,
}

impl MenuName {
    pub const ALL: [MenuName; 3] = [MenuName::App, MenuName::View, MenuName::Tools];

    pub fn title(self) -> &'static str {
        match self {
            MenuName::App => "Natamux",
            MenuName::View => "View",
            MenuName::Tools => "Tools",
        }
    }
}

// `when` gates the command palette of a later screen.
#[cfg_attr(not(test), allow(dead_code))]
/// What the view knows when a key comes (`CommandState`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CommandState {
    /// A task fills the main area (the terminal pane until screen 2).
    pub task_open: bool,
    pub has_selection: bool,
    pub selected_state: Option<TaskState>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum When {
    Always,
    /// The task list is on screen, not an open task.
    ListShown,
    HasSelection,
    /// The selected task's state offers this event without asking first.
    TaskAction(&'static str),
}

impl When {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn holds(self, state: &CommandState) -> bool {
        match self {
            When::Always => true,
            When::ListShown => !state.task_open,
            When::HasSelection => state.has_selection,
            When::TaskAction(event) => state.selected_state.is_some_and(|selected| {
                actions_for(selected)
                    .iter()
                    .any(|action| action.event_type == event && !action.confirm)
            }),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Command {
    pub id: &'static str,
    pub title: &'static str,
    pub category: Category,
    /// As the web writes them, such as `Mod+B` (Mod is cmd on macOS, ctrl elsewhere). The first
    /// one shows in menus and tooltips.
    pub keys: &'static [&'static str],
    /// Where the keys work. Empty: everywhere.
    pub contexts: &'static [&'static str],
    pub when: When,
    /// The menu it shows in, and its section there.
    pub menu: Option<(MenuName, u8)>,
}

const fn command(id: &'static str, title: &'static str, category: Category) -> Command {
    Command {
        id,
        title,
        category,
        keys: &[],
        contexts: &[],
        when: When::Always,
        menu: None,
    }
}

const fn keys(
    command: Command,
    keys: &'static [&'static str],
    contexts: &'static [&'static str],
) -> Command {
    Command {
        keys,
        contexts,
        ..command
    }
}

const fn when(command: Command, when: When) -> Command {
    Command { when, ..command }
}

const fn menu(command: Command, name: MenuName, section: u8) -> Command {
    Command {
        menu: Some((name, section)),
        ..command
    }
}

const fn task_action(id: &'static str, title: &'static str, event: &'static str) -> Command {
    when(command(id, title, Category::Task), When::TaskAction(event))
}

use Category::{App, Task, Tools, View};

/// Screen 1 and the Tools menu. Ids are the web's where the web has the command.
pub const COMMANDS: &[Command] = &[
    menu(
        keys(command("app.quit", "Quit Natamux", App), &["Mod+Q"], &[]),
        MenuName::App,
        0,
    ),
    // Global, but a terminal keeps Esc for its agent (owner, 2026-10-22): `terminal_keeps`.
    keys(
        command(
            "escape",
            "Close the top layer, clear a field, or go back to All tasks",
            App,
        ),
        &["Escape"],
        &[],
    ),
    keys(
        command("form.submit", "Submit the current form", App),
        &["Mod+Enter"],
        &[DIALOG],
    ),
    // The way back from a task's terminal, which keeps Esc. Off macOS Ctrl+[ is Esc in a shell,
    // so there only the menu and the sidebar go back.
    menu(
        keys(command("view.tasks", "All tasks", View), &["Mod+["], &[]),
        MenuName::View,
        0,
    ),
    menu(command("view.agents", "Agents", View), MenuName::View, 0),
    menu(
        command("view.table", "Show tasks as a table", View),
        MenuName::View,
        1,
    ),
    menu(
        command("view.board", "Show tasks as a board", View),
        MenuName::View,
        1,
    ),
    menu(
        keys(
            command("sidebar.toggle", "Show or hide the sidebar", View),
            &["Mod+B"],
            &[],
        ),
        MenuName::View,
        2,
    ),
    keys(
        command("filter.focus", "Search the sidebar", View),
        &["/"],
        SINGLE,
    ),
    keys(
        command("workspace.new", "Start a task", Task),
        &["c"],
        SINGLE,
    ),
    keys(command("task.new", "Add a task", Task), &["n"], SINGLE),
    when(
        keys(
            command("list.next", "Select the next task", Task),
            &["j", "ArrowDown"],
            SINGLE,
        ),
        When::ListShown,
    ),
    when(
        keys(
            command("list.previous", "Select the previous task", Task),
            &["k", "ArrowUp"],
            SINGLE,
        ),
        When::ListShown,
    ),
    // The web's `list.ends` and `tree.collapse` are one entry for two keys; here each key has
    // its own action.
    keys(command("list.first", "First row", App), &["Home"], &[LIST]),
    keys(command("list.last", "Last row", App), &["End"], &[LIST]),
    when(
        keys(
            command("task.open", "Open the selected task", Task),
            &["Enter"],
            SINGLE,
        ),
        When::HasSelection,
    ),
    keys(
        command("tree.collapse", "Collapse a group", App),
        &["ArrowLeft"],
        &[TREE],
    ),
    keys(
        command("tree.expand", "Expand a group", App),
        &["ArrowRight"],
        &[TREE],
    ),
    keys(
        command("row.rename", "Rename the workspace", App),
        &["F2"],
        &[TREE],
    ),
    keys(
        command("row.menu", "Open the menu of the row", App),
        &["Shift+F10", "ContextMenu"],
        &[LIST],
    ),
    keys(
        command("row.remove", "Remove the workspace (asks first)", App),
        &["Delete"],
        &[TREE],
    ),
    // Menu items with no key of their own: the row, project and table menus run them; the key
    // of an item that has one comes from its command above.
    command("task.add-subtask", "Add subtask\u{2026}", Task),
    command("row.pin", "Pin to the top", Task),
    command("row.unpin", "Unpin", Task),
    command("row.copy-branch", "Copy branch name", Task),
    command("row.copy-path", "Copy folder path", Task),
    command("task.copy-id", "Copy task id", Task),
    command("agent.start", "Start agent", Task),
    command("agent.restart", "Restart agent", Task),
    command("task.cancel", "Cancel task", Task),
    command("task.delete", "Delete task", Task),
    command("project.blur", "Blur names", App),
    command("project.show-names", "Show names", App),
    command("project.remove-finished", "Remove finished workspaces", App),
    command("project.remove", "Remove project", App),
    task_action("task.task.readied", "Mark ready", "task.readied"),
    task_action("task.task.blocked", "Block", "task.blocked"),
    task_action("task.agent.resumed", "Resume the agent", "agent.resumed"),
    keys(
        task_action("task.review.approved", "Approve", "review.approved"),
        &["a"],
        SINGLE,
    ),
    keys(
        task_action(
            "task.review.changes-requested",
            "Request changes",
            "review.changes-requested",
        ),
        &["r"],
        SINGLE,
    ),
    task_action("task.task.unblocked", "Unblock", "task.unblocked"),
    task_action("task.task.retried", "Retry", "task.retried"),
    // Screen 2: the panes of a workspace and the Agents grid (`paneCommands`).
    keys(
        command(
            "pane.maximize",
            "Maximize or restore the focused pane",
            View,
        ),
        &["Mod+Shift+Enter"],
        &[],
    ),
    keys(
        command("shell.new", "New terminal", Task),
        &["Ctrl+Shift+`"],
        &[],
    ),
    // Inside a terminal only, so the shell keeps Ctrl+F elsewhere.
    keys(
        command("terminal.find", "Find in the terminal", View),
        &["Mod+F", "Mod+Shift+F"],
        &[TERMINAL],
    ),
    // Esc in the find bar is the global `escape`: the bar takes it before the shell.
    keys(
        command("find.next", "Next match", View),
        &["Enter"],
        &[TERMINAL_FIND],
    ),
    keys(
        command("find.previous", "Previous match", View),
        &["Shift+Enter"],
        &[TERMINAL_FIND],
    ),
    menu(
        keys(
            command("tools.perf-overlay", "Toggle Perf Overlay", Tools),
            &["Mod+Alt+P"],
            &[],
        ),
        MenuName::Tools,
        0,
    ),
    menu(
        keys(
            command("tools.save-perf", "Save Perf Report", Tools),
            &["Mod+Alt+S"],
            &[],
        ),
        MenuName::Tools,
        0,
    ),
    menu(
        keys(
            command("tools.bench", "Bench: 10 Terminals", Tools),
            &["Mod+Alt+B"],
            &[],
        ),
        MenuName::Tools,
        1,
    ),
    menu(
        keys(
            command("tools.stop-bench", "Stop Bench", Tools),
            &["Mod+Alt+X"],
            &[],
        ),
        MenuName::Tools,
        1,
    ),
    menu(
        keys(
            command("tools.ime-check", "IME Check", Tools),
            &["Mod+Alt+I"],
            &[],
        ),
        MenuName::Tools,
        2,
    ),
    menu(
        keys(
            command("tools.close", "Back to the Terminal", Tools),
            &["Mod+Alt+W"],
            &[],
        ),
        MenuName::Tools,
        2,
    ),
    keys(
        command("tools.ime-submit", "Check the typed word", Tools),
        &["Enter"],
        &[IME_CHECK],
    ),
];

pub fn command_by_id(id: &str) -> Option<&'static Command> {
    COMMANDS.iter().find(|command| command.id == id)
}

/// The title of a command, for a menu item.
pub fn title_of(id: &'static str) -> &'static str {
    command_by_id(id).map_or(id, |command| command.title)
}

/// The first key of a command, for a tooltip or a menu row (`shortcutOf`).
pub fn shortcut_of(id: &str) -> Option<&'static str> {
    command_by_id(id)?.keys.first().copied()
}

/// The commands of one menu, in registry order, with their section.
pub fn menu_items(name: MenuName) -> impl Iterator<Item = (&'static Command, u8)> {
    COMMANDS
        .iter()
        .filter_map(move |command| match command.menu {
            Some((menu, section)) if menu == name => Some((command, section)),
            _ => None,
        })
}

// How shortcuts are written and read: `app/shortcut_text.rs`.
#[cfg(test)]
pub use super::shortcut_text::key_text;
pub use super::shortcut_text::{is_single_key, keystroke, reaches_app_from_terminal};

#[cfg(test)]
#[path = "commands_tests.rs"]
mod tests;
