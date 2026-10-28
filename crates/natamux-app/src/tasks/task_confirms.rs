//! The Delete task and Cancel task confirms of the row menus (`TaskTable.tsx`, `WorkspaceRow.tsx`),
//! and the board's Delete all done or cancelled (`TaskBoard.tsx`).

use gpui::Entity;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::task_rules::{
    actions_for, cancel_task_description, delete_task_description, delete_task_label,
};
use super::task_view_rules::{delete_all_description, delete_all_label, delete_all_title};
use crate::app::dashboard::Dashboard;
use crate::app::source::read_uncommitted;
use crate::components::confirm::Confirm;

/// Asks before the task, its notes, its history and its worktree are deleted. The files that are
/// not committed are read first: the confirm names them, and only then the delete discards them.
pub fn confirm_delete_task(
    dashboard: &Entity<Dashboard>,
    task: &Task,
    window: &mut Window,
    cx: &mut App,
) {
    let source = dashboard.read(cx).source().cloned();
    let (Some(source), Some(_)) = (source, &task.workspace) else {
        open_delete_task(dashboard, &task.id, 0, window, cx);
        return;
    };
    let changes = source.list_changes(&task.id);
    let dashboard = dashboard.clone();
    let id = task.id.clone();
    let main = window.window_handle();
    cx.spawn(async move |cx| {
        // A failed read counts as none: the daemon then still refuses changes.
        let count = changes.await.map_or(0, |files| files.len());
        main.update(cx, |_, window, cx| {
            open_delete_task(&dashboard, &id, count, window, cx);
        })
        .ok();
    })
    .detach();
}

fn open_delete_task(
    dashboard: &Entity<Dashboard>,
    id: &str,
    uncommitted: usize,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let task_id = id.to_string();
    Confirm::new(
        format!("Delete {id}?"),
        delete_task_description(uncommitted),
        delete_task_label(uncommitted),
        move |_, cx| {
            dashboard.update(cx, |dashboard, cx| {
                dashboard
                    .delete_task(&task_id, uncommitted > 0, cx)
                    .detach()
            });
        },
    )
    .open(window, cx);
}

/// Asks before the task is cancelled. Does nothing for a state that has no Cancel.
pub fn confirm_cancel_task(
    dashboard: &Entity<Dashboard>,
    task: &Task,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(cancel) = actions_for(task.state)
        .into_iter()
        .find(|action| action.confirm)
    else {
        return;
    };
    let dashboard = dashboard.clone();
    let id = task.id.clone();
    Confirm::new(
        format!("Cancel {}?", task.id),
        cancel_task_description(task.workspace.is_some()),
        "Cancel task",
        move |_, cx| {
            dashboard.update(cx, |dashboard, cx| {
                dashboard.send_event(&id, cancel.event_type, cx).detach()
            });
        },
    )
    .cancel_label("Keep working")
    .open(window, cx);
}

/// Reads the files that are not committed of the `tasks` with a workspace, then calls `open`
/// with (task id, file count) of those that have any. A bulk confirm names them, and only those
/// are then forced.
pub fn with_uncommitted(
    dashboard: &Entity<Dashboard>,
    tasks: &[Task],
    window: &mut Window,
    cx: &mut App,
    open: impl FnOnce(Vec<(String, usize)>, &mut Window, &mut App) + 'static,
) {
    let Some(source) = dashboard.read(cx).source().cloned() else {
        open(Vec::new(), window, cx);
        return;
    };
    let ids: Vec<String> = tasks
        .iter()
        .filter(|task| task.workspace.is_some())
        .map(|task| task.id.clone())
        .collect();
    let reads = read_uncommitted(&source, &ids);
    let main = window.window_handle();
    cx.spawn(async move |cx| {
        let dirty = reads.await;
        main.update(cx, |_, window, cx| open(dirty, window, cx))
            .ok();
    })
    .detach();
}

/// The board's Delete all: the tasks shown in the column when it was clicked, one after the
/// other, then one summary toast.
pub fn confirm_delete_all(
    dashboard: &Entity<Dashboard>,
    state: TaskState,
    tasks: &[Task],
    window: &mut Window,
    cx: &mut App,
) {
    let board = dashboard.clone();
    let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let rows: Vec<(SharedString, SharedString)> = tasks
        .iter()
        .map(|task| (task.id.clone().into(), task.title.clone().into()))
        .collect();
    let title = delete_all_title(state, tasks.len());
    with_uncommitted(dashboard, tasks, window, cx, move |dirty, window, cx| {
        let force: Vec<String> = dirty.iter().map(|(id, _)| id.clone()).collect();
        Confirm::new(
            title,
            delete_all_description(&dirty),
            delete_all_label(!dirty.is_empty()),
            move |_, cx| {
                let (ids, force) = (ids.clone(), force.clone());
                board.update(cx, |board, cx| board.delete_tasks(ids, force, cx));
            },
        )
        .rows(rows)
        .open(window, cx);
    });
}

/// Keep this try (`CompareTries.tsx`): the other tries that are not finished are cancelled.
pub fn confirm_keep_try(
    dashboard: &Entity<Dashboard>,
    kept: &Task,
    others: Vec<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    Confirm::new(
        format!("Keep {}?", kept.id),
        "The other tries that are not finished are cancelled. Their workspaces and branches are kept.",
        "Keep this try, cancel the others",
        move |_, cx| {
            let others = others.clone();
            dashboard.update(cx, |dashboard, cx| dashboard.keep_try(others, cx));
        },
    )
    .open(window, cx);
}
