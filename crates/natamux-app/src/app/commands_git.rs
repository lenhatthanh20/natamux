//! Screen 4's commands: the workspace tabs and the Changes tab (`view.ts` of the web). Same
//! registry as `commands.rs`; `all_commands` joins the two lists.

use super::commands::{
    BODY, CHANGES, CHANGES_ANY, COMMIT_BOX, Category, Command, MenuName, When, command, keys, menu,
    when,
};
use super::commands_files::DOCS;

/// Single keys of the workspace: its body, its Changes and History tabs, and Docs.
const TAB_KEYS: &[&str] = &[BODY, CHANGES, DOCS];

const fn in_workspace(command: Command) -> Command {
    when(command, When::InWorkspace)
}

const fn tab(id: &'static str, title: &'static str, key: &'static [&'static str]) -> Command {
    in_workspace(keys(command(id, title, Category::View), key, TAB_KEYS))
}

/// Key 4 (Docs) is in `commands_files.rs`.
pub const GIT_COMMANDS: &[Command] = &[
    tab("tab.terminal", "Show the Terminal tab", &["1"]),
    tab("tab.changes", "Show the Changes tab", &["2"]),
    tab("tab.history", "Show the History tab", &["3"]),
    in_workspace(keys(
        command("changes.refresh", "Refresh the changes", Category::Task),
        &["Shift+R"],
        &[CHANGES],
    )),
    in_workspace(keys(
        command("diff.prev-file", "Previous changed file", Category::Task),
        &["["],
        &[CHANGES],
    )),
    in_workspace(keys(
        command("diff.next-file", "Next changed file", Category::Task),
        &["]"],
        &[CHANGES],
    )),
    in_workspace(keys(
        command(
            "diff.next-change",
            "Next change in the file",
            Category::Task,
        ),
        &["Alt+F5"],
        &[CHANGES_ANY],
    )),
    in_workspace(keys(
        command(
            "diff.prev-change",
            "Previous change in the file",
            Category::Task,
        ),
        &["Shift+Alt+F5"],
        &[CHANGES_ANY],
    )),
    in_workspace(keys(
        command(
            "diff.comment",
            "Comment on the file or the selection for the agent",
            Category::Task,
        ),
        &["Mod+Shift+A"],
        &[CHANGES_ANY],
    )),
    in_workspace(keys(
        command(
            "changes.rollback",
            "Roll back the shown file",
            Category::Task,
        ),
        &["Mod+Alt+Z"],
        &[CHANGES_ANY],
    )),
    // Only in the commit box: elsewhere the key would start an agent run, and its draft could
    // replace text the user did not mean to give up.
    in_workspace(keys(
        command(
            "diff.draft-commit",
            "Draft the commit message with the agent",
            Category::Task,
        ),
        &["Mod+Shift+G"],
        &[COMMIT_BOX],
    )),
    menu(
        in_workspace(command("task.commit", "Commit\u{2026}", Category::Task)),
        MenuName::Task,
        0,
    ),
];
