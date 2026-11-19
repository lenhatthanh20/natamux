//! The edit items of a code view's right-click menu (editor tab, diff sides): Cut, Copy, Paste
//! and Select All on the text. The blame items of the row follow them (`git/blame_marks.rs`).

use gpui::{Action, Entity, FocusHandle};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::text_input::actions as own;
use crate::git::code_target::RowMenu;

/// What the edit items need of the text input.
pub(super) struct EditState {
    focus: FocusHandle,
    selected: bool,
    read_only: bool,
}

impl EditState {
    pub(super) fn new(focus: FocusHandle, selected: bool, read_only: bool) -> Self {
        Self {
            focus,
            selected,
            read_only,
        }
    }
}

/// The whole menu of a right press on `row`: the edit items, then the blame items when blame is
/// attached.
pub(super) fn row_menu(
    edits: EditState,
    blame: Option<RowMenu>,
    row: Option<u32>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<ContextMenu> {
    ContextMenu::build(window, cx, move |menu, window, cx| {
        let menu = fill_edit_items(menu, &edits.focus, edits.selected, edits.read_only);
        match blame {
            Some(blame) => blame(row, menu, window, cx),
            None => menu,
        }
    })
}

/// One edit item of the menu.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct EditItem {
    pub label: &'static str,
    pub enabled: bool,
}

/// Cut and Copy need a selection; Cut and Paste need a text that can change.
pub(super) fn edit_items(selected: bool, read_only: bool) -> [EditItem; 4] {
    [
        EditItem {
            label: "Cut",
            enabled: selected && !read_only,
        },
        EditItem {
            label: "Copy",
            enabled: selected,
        },
        EditItem {
            label: "Paste",
            enabled: !read_only,
        },
        EditItem {
            label: "Select All",
            enabled: true,
        },
    ]
}

fn action_of(label: &str) -> Box<dyn Action> {
    match label {
        "Cut" => Box::new(own::Cut),
        "Copy" => Box::new(own::Copy),
        "Paste" => Box::new(own::Paste),
        _ => Box::new(own::SelectAll),
    }
}

/// Adds the edit items. Each sends its action to the text input at `focus`, so the menu shows
/// the input's keys and the input does the work as for the keys.
fn fill_edit_items(
    menu: ContextMenu,
    focus: &FocusHandle,
    selected: bool,
    read_only: bool,
) -> ContextMenu {
    let mut menu = menu.context(focus.clone());
    for item in edit_items(selected, read_only) {
        if item.label == "Select All" {
            menu = menu.separator();
        }
        let shown = action_of(item.label);
        let sent = shown.boxed_clone();
        let focus = focus.clone();
        let entry = ContextMenuEntry::new(item.label)
            .action(shown)
            .disabled(!item.enabled)
            .handler(move |window, cx| {
                window.focus(&focus, cx);
                window.dispatch_action(sent.boxed_clone(), cx);
            });
        menu = menu.item(entry);
    }
    menu
}

#[cfg(test)]
#[path = "code_menu_tests.rs"]
mod tests;
