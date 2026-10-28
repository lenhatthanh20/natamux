//! The Delete task and Cancel task confirms of the row menus (`TaskTable.tsx`, `WorkspaceRow.tsx`),
//! and the board's Delete all done or cancelled (`TaskBoard.tsx`).

use gpui::Entity;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::task_rules::{
    actions_for, cancel_task_description, delete_task_description, delete_task_label,
};
use super::task_view_rules::{DELETE_ALL_DESCRIPTION, delete_all_title};
use crate::app::dashboard::Dashboard;
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

/// The board's Delete all: the tasks shown in the column when it was clicked, one after the
/// other, then one summary toast.
pub fn confirm_delete_all(
    dashboard: &Entity<Dashboard>,
    state: TaskState,
    tasks: &[Task],
    window: &mut Window,
    cx: &mut App,
) {
    let dashboard = dashboard.clone();
    let ids: Vec<String> = tasks.iter().map(|task| task.id.clone()).collect();
    let rows = tasks
        .iter()
        .map(|task| (task.id.clone().into(), task.title.clone().into()))
        .collect();
    Confirm::new(
        delete_all_title(state, tasks.len()),
        DELETE_ALL_DESCRIPTION,
        "Delete",
        move |_, cx| {
            let ids = ids.clone();
            dashboard.update(cx, |dashboard, cx| dashboard.delete_tasks(ids, cx));
        },
    )
    .rows(rows)
    .open(window, cx);
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
