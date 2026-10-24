//! Screen 6's commands: the Docs tab, the open file and the files of the worktree (`view.ts` of
//! the web), Save in the editor tab, and closing and reopening the tabs. Same registry as `commands.rs`; `all_commands` joins the
//! lists. The actions are declared here; `shortcuts.rs` maps the ids to them.

use gpui::actions;

use super::commands::{
    BODY, CHANGES, Category, Command, MenuName, SheetGroup, When, command, group, hidden, keys,
    menu, when,
};

actions!(
    natamux,
    [
        ShowDocsTab,
        ShowOpenFile,
        ShowWorktreeFiles,
        SaveFile,
        CloseTab,
        ReopenTabs
    ]
);

/// The Docs tab, while none of its text boxes has the focus.
pub const DOCS: &str = "NatamuxDocs && not_editing";
/// The editor tab: Save works in its editor.
pub const FILE_EDITOR: &str = "NatamuxFileEditor";

const fn in_workspace(command: Command) -> Command {
    when(command, When::InWorkspace)
}

/// No command takes 5: it showed the Docs tab before, and the open file has no key.
pub const FILE_COMMANDS: &[Command] = &[
    in_workspace(keys(
        group(
            command("tab.docs", "Show the Docs tab", Category::View),
            SheetGroup::SingleKeys,
        ),
        &["4"],
        &[BODY, CHANGES, DOCS],
    )),
    menu(
        in_workspace(command("tab.editor", "Show the open file", Category::View)),
        MenuName::View,
        3,
    ),
    menu(
        in_workspace(command(
            "files.show",
            "Show the files of the worktree",
            Category::Workspace,
        )),
        MenuName::View,
        3,
    ),
    in_workspace(keys(
        hidden(command("file.save", "Save the file", Category::Task)),
        &["Mod+S"],
        &[FILE_EDITOR],
    )),
    // T-0158: the workspace tabs close as Zed's pane tabs. Terminal stays.
    in_workspace(keys(
        group(
            command("tab.close", "Close the tab", Category::View),
            SheetGroup::Global,
        ),
        &["Mod+W"],
        &[],
    )),
    in_workspace(command(
        "tab.reopen",
        "Reopen the closed tabs",
        Category::View,
    )),
];
