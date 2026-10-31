//! The row menu of a workspace and its commands (docs/ui-spec.md 4.9): Answer (needs input), Rename,
//! Add subtask…, Pin to the top / Unpin, Copy branch name, Copy folder path, Remove workspace,
//! Delete task. The items are `sidebar_text::row_menu`; the dialogs of other screens open through
//! their `open`.
//! The project menu opens here too; `project_header` runs its commands.

use std::rc::Rc;

use gpui::{ClipboardItem, Pixels, Point};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::workspace_tree::WorkspaceTree;
use crate::app::commands::shortcut_of;
use crate::app::dashboard_data::BulkRun;
use crate::app::shortcuts::action_for;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;
use crate::components::toast_rules::ToastTone;
use crate::sidebar::sidebar_text::{
    MenuItem, ProjectCommand, ProjectMenuInput, RowCommand, project_menu, row_menu,
};
use crate::tasks::add_subtask_dialog;
use crate::tasks::task_confirms::confirm_delete_task;
use crate::workspaces::remove_workspace_dialog;

/// Runs the command of a picked menu item.
pub(super) type RunCommand<C> = Rc<dyn Fn(C, &mut Window, &mut App)>;

/// Adds the items to a menu. `run` gets the command of the picked item.
pub(super) fn fill_menu<C: Copy + 'static>(
    mut menu: ContextMenu,
    items: Vec<MenuItem<C>>,
    run: RunCommand<C>,
) -> ContextMenu {
    for item in items {
        let MenuItem::Item {
            id,
            label,
            icon,
            command,
            danger,
            disabled,
            hint,
        } = item
        else {
            menu = menu.separator();
            continue;
        };
        let run = run.clone();
        let mut entry = ContextMenuEntry::new(label)
            .custom_icon_path(icon_path(icon))
            .icon_position(IconPosition::Start)
            .icon_color(if danger { Color::Error } else { Color::Muted })
            .disabled(disabled)
            .handler(move |window, cx| run(command, window, cx));
        // The menu shows the key of the item's command: F2 for Rename, Delete for Remove.
        if let Some(action) = shortcut_of(id).and_then(|_| action_for(id)) {
            entry = entry.action(action);
        }
        menu = menu.item(entry);
        if let Some(hint) = hint {
            menu = menu.label(hint);
        }
    }
    menu
}

impl WorkspaceTree {
    /// Opens the menu of a task row at `position`: the pointer, or under the row for Shift+F10.
    pub(super) fn open_row_menu(
        &mut self,
        task_id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dashboard = self.dashboard.read(cx);
        let Some(task) = dashboard.data.task(task_id) else {
            return;
        };
        let pinned = dashboard.view().pinned.iter().any(|id| id == task_id);
        let items = row_menu(task, pinned);
        let tree = cx.weak_entity();
        let id = task_id.to_owned();
        let run: RunCommand<RowCommand> = Rc::new(move |command, window, cx| {
            tree.update(cx, |tree, cx| {
                tree.run_row_command(&id, command, window, cx)
            })
            .ok();
        });
        let focus = self.focus.clone();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            fill_menu(menu.context(focus), items, run)
        });
        MenuLayer::open(menu, position, window, cx);
    }

    /// Opens the project menu at `position`: the pointer, or under the header for Shift+F10.
    pub(super) fn open_project_menu(
        &mut self,
        repo_id: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(model) = self.model.as_ref() else {
            return;
        };
        let Some(group) = model
            .groups
            .groups
            .iter()
            .position(|group| group.repo.id == repo_id)
        else {
            return;
        };
        let info = &model.infos[group];
        let input = ProjectMenuInput {
            known: !info.gone,
            blurred: self
                .dashboard
                .read(cx)
                .view()
                .blurred
                .iter()
                .any(|id| id == repo_id),
            finished: model.groups.groups[group].finished.len(),
            removing_finished: self
                .dashboard
                .read(cx)
                .bulk
                .running(BulkRun::RemoveFinished),
            workspaces: info.workspaces,
        };
        let items = project_menu(input);
        let tree = cx.weak_entity();
        let repo = repo_id.to_owned();
        let run: RunCommand<ProjectCommand> = Rc::new(move |command, window, cx| {
            tree.update(cx, |tree, cx| {
                tree.run_project_command(&repo, command, window, cx)
            })
            .ok();
        });
        let focus = self.focus.clone();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            fill_menu(menu.context(focus), items, run)
        });
        MenuLayer::open(menu, position, window, cx);
    }

    fn run_row_command(
        &mut self,
        task_id: &str,
        command: RowCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.dashboard.read(cx).data.task(task_id).cloned() else {
            return;
        };
        match command {
            RowCommand::Answer => self.answer(task_id, cx),
            RowCommand::ViewChanges => self.dashboard.update(cx, |dashboard, cx| {
                dashboard.open_changes(task_id, false, cx)
            }),
            RowCommand::Rename => self.start_rename(task_id, window, cx),
            RowCommand::AddSubtask => add_subtask_dialog::open(task, window, cx),
            RowCommand::TogglePin => self
                .dashboard
                .update(cx, |dashboard, cx| dashboard.toggle_pin(task_id, cx)),
            RowCommand::CopyBranch => {
                self.copy(task.branch.unwrap_or_default(), "branch name", cx);
            }
            RowCommand::CopyFolder => {
                self.copy(task.workspace.unwrap_or_default(), "folder path", cx);
            }
            RowCommand::RemoveWorkspace => self.remove_workspace(task_id, window, cx),
            RowCommand::DeleteTask => confirm_delete_task(&self.dashboard, &task, window, cx),
        }
    }

    /// Delete and the menu item: the dialog that asks before the workspace is removed.
    pub(super) fn remove_workspace(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = self.dashboard.read(cx).data.task(task_id).cloned();
        if let Some(task) = task.filter(|task| task.workspace.is_some()) {
            remove_workspace_dialog::open(&self.dashboard, task, window, cx);
        }
    }

    pub(super) fn copy(&mut self, text: String, what: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let said = format!("Copied the {what}.");
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.toast(&said, ToastTone::Info, None, cx)
        });
    }
}
