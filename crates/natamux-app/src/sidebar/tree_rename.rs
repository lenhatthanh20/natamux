//! The inline rename of a sidebar row (F2 or the row menu): a one-line editor in place of the
//! title. Enter or a blur saves, Esc drops.

use editor::{Editor, EditorEvent};
use gpui::{Entity, Focusable, Subscription};
use ui::prelude::*;

use crate::sidebar::sidebar_text::renamed_title;
use crate::sidebar::workspace_tree::WorkspaceTree;

/// The inline rename of a row: a one-line editor in place of the title.
pub(super) struct Rename {
    pub(super) task_id: String,
    pub(super) editor: Entity<Editor>,
    pub(super) _blur: Subscription,
}

impl WorkspaceTree {
    pub(super) fn start_rename(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.dashboard.read(cx).data.task(task_id).cloned() else {
            return;
        };
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(task.title.clone(), window, cx);
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor
        });
        // Leaving the field keeps the name, as Enter does.
        let blur = cx.subscribe_in(
            &editor,
            window,
            |tree, _, event: &EditorEvent, window, cx| {
                if matches!(event, EditorEvent::Blurred) {
                    tree.finish_rename(true, false, window, cx);
                }
            },
        );
        window.focus(&editor.focus_handle(cx), cx);
        self.active = Some(format!("task:{task_id}"));
        self.rename = Some(Rename {
            task_id: task_id.to_owned(),
            editor,
            _blur: blur,
        });
        cx.notify();
    }

    /// `keep`: Enter or a blur saves, Esc drops. `refocus`: focus goes back to the row.
    pub(super) fn finish_rename(
        &mut self,
        keep: bool,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.rename.take() else {
            return;
        };
        let draft = rename.editor.read(cx).text(cx);
        let title = self
            .dashboard
            .read(cx)
            .data
            .task(&rename.task_id)
            .map(|task| task.title.clone())
            .unwrap_or_default();
        if keep && let Some(title) = renamed_title(&draft, &title) {
            self.dashboard
                .update(cx, |dashboard, cx| {
                    dashboard.rename_task(&rename.task_id, &title, cx)
                })
                .detach();
        }
        if refocus {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }
}

#[cfg(test)]
#[path = "tree_rename_tests.rs"]
mod tests;
