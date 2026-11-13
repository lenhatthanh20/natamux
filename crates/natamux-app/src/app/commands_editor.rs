//! The keys of the full editors (the diff, the editor tab, the Docs edit box), T-0316. Most are
//! Zed editor actions: they are listed here for the shortcut sheet, and `text_boxes.rs` binds each
//! id to its Zed action with these keys and contexts. Find, Go to line and Go to symbol are the
//! app's own actions (`shortcuts.rs`).

use gpui::actions;

use super::commands::{
    Category, Command, DIFF_EDITOR, SheetGroup, When, command, group, hidden, keys, when,
};

actions!(natamux, [FindInEditor, GoToLine, ShowOutline]);

/// Every full editor: Zed's `Editor` in `mode == full`. Single-line and auto-height boxes are not.
pub const FULL_EDITOR: &str = "Editor && mode == full";
/// The editor of the editor tab only.
pub const FILE_TEXT: &str = "NatamuxFileEditor > Editor";
/// The diff view and the editor tab, around their editors.
pub const DIFF_VIEW: &str = "NatamuxDiff";
pub const FILE_VIEW: &str = "NatamuxFileEditor";
/// The find bar of the diff and the editor tab (`git/editor_find.rs`).
pub const EDITOR_FIND: &str = "EditorFind";

const fn editor_key(
    id: &'static str,
    title: &'static str,
    keys_of: &'static [&'static str],
    contexts: &'static [&'static str],
) -> Command {
    Command {
        documents_only: true,
        ..hidden(group(
            keys(command(id, title, Category::View), keys_of, contexts),
            SheetGroup::Editor,
        ))
    }
}

const fn in_workspace(command: Command) -> Command {
    when(command, When::InWorkspace)
}

pub const EDITOR_COMMANDS: &[Command] = &[
    in_workspace(keys(
        group(
            command("editor.find", "Find in the file", Category::View),
            SheetGroup::Editor,
        ),
        &["Mod+F"],
        &[DIFF_VIEW, FILE_VIEW],
    )),
    keys(
        hidden(command("editor.find-next", "Next match", Category::View)),
        &["Enter"],
        &[EDITOR_FIND],
    ),
    keys(
        hidden(command(
            "editor.find-previous",
            "Previous match",
            Category::View,
        )),
        &["Shift+Enter"],
        &[EDITOR_FIND],
    ),
    // Ctrl on every platform: Cmd+G is find next on macOS.
    in_workspace(keys(
        group(
            command("editor.go-to-line", "Go to line", Category::View),
            SheetGroup::Editor,
        ),
        &["Ctrl+G"],
        &[DIFF_VIEW, FILE_VIEW],
    )),
    in_workspace(keys(
        group(
            command("editor.outline", "Go to symbol", Category::View),
            SheetGroup::Editor,
        ),
        &["Mod+Shift+O"],
        &[DIFF_VIEW, FILE_VIEW],
    )),
    editor_key(
        "editor.select-next",
        "Select the next match",
        &["Mod+D"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.select-all-matches",
        "Select all matches",
        &["Mod+Shift+L"],
        &[FULL_EDITOR],
    ),
    // Not Ctrl+Shift+Left/Right: off macOS those select a word.
    editor_key(
        "editor.select-larger",
        "Expand the selection to the syntax node",
        &["Ctrl+Shift+ArrowUp"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.select-smaller",
        "Shrink the selection to the syntax node",
        &["Ctrl+Shift+ArrowDown"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.add-cursor-above",
        "Add a cursor above",
        &["Mod+Alt+ArrowUp"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.add-cursor-below",
        "Add a cursor below",
        &["Mod+Alt+ArrowDown"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.move-line-up",
        "Move the line up",
        &["Alt+ArrowUp"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.move-line-down",
        "Move the line down",
        &["Alt+ArrowDown"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.duplicate-line",
        "Duplicate the line",
        &["Alt+Shift+ArrowDown"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.delete-line",
        "Delete the line",
        &["Mod+Shift+K"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.toggle-comments",
        "Comment or uncomment the lines",
        &["Mod+/"],
        &[FULL_EDITOR],
    ),
    editor_key(
        "editor.toggle-fold",
        "Fold or unfold the block",
        &["Mod+Alt+["],
        &[FILE_TEXT],
    ),
    editor_key(
        "editor.fold-all",
        "Fold all blocks",
        &["Mod+Alt+Shift+["],
        &[FILE_TEXT],
    ),
    editor_key(
        "editor.unfold-all",
        "Unfold all blocks",
        &["Mod+Alt+Shift+]"],
        &[FILE_TEXT],
    ),
    // Inline only: side by side keeps every change open.
    editor_key(
        "editor.toggle-hunk",
        "Collapse or expand the change (inline diff)",
        &["Mod+'"],
        &[DIFF_EDITOR],
    ),
];

#[cfg(test)]
#[path = "commands_editor_tests.rs"]
mod tests;
