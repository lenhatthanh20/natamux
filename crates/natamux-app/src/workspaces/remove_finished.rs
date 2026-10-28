//! The Remove finished workspaces confirm of a project (`ClearFinishedDialog.tsx`). The removal
//! runs one workspace after the other and ends with one summary toast
//! (`Dashboard::remove_workspaces`, rules in `remove_finished_rules.rs`). Workspaces with files
//! that are not committed are named first, then removed anyway.

use gpui::Entity;
use ui::prelude::*;

use super::remove_dialog_rules::{
    remove_finished_description, remove_finished_label, remove_finished_title,
};
use crate::app::dashboard::Dashboard;
use crate::components::confirm::Confirm;
use crate::sidebar::workspace_groups::{RepoRef, workspace_groups};
use crate::tasks::task_confirms::with_uncommitted;

/// Asks before the finished workspaces of the project are removed: the ones its sidebar group
/// folds into "N finished". Does nothing when it has none. It reads their files that are not
/// committed first, names them, and removes those anyway.
pub fn open(dashboard: &Entity<Dashboard>, repo_id: String, window: &mut Window, cx: &mut App) {
    let board = dashboard.read(cx);
    let repos: Vec<RepoRef> = board
        .data
        .repos
        .iter()
        .map(|repo| RepoRef {
            id: repo.id.clone(),
            name: repo.name.clone(),
        })
        .collect();
    let groups = workspace_groups(&repos, &board.data.tasks, &board.view().pinned, None);
    let Some(group) = groups
        .groups
        .into_iter()
        .find(|group| group.repo.id == repo_id)
    else {
        return;
    };
    if group.finished.is_empty() {
        return;
    }
    let ids: Vec<String> = group.finished.iter().map(|task| task.id.clone()).collect();
    let rows: Vec<(SharedString, SharedString)> = group
        .finished
        .iter()
        .map(|task| (task.id.clone().into(), task.title.clone().into()))
        .collect();
    let title = remove_finished_title(ids.len());
    let repo_name = group.repo.name;
    let board = dashboard.clone();
    with_uncommitted(
        dashboard,
        &group.finished,
        window,
        cx,
        move |dirty, window, cx| {
            let force: Vec<String> = dirty.iter().map(|(id, _)| id.clone()).collect();
            Confirm::new(
                title,
                remove_finished_description(&repo_name, &dirty),
                remove_finished_label(!dirty.is_empty()),
                move |_, cx| {
                    let (ids, force) = (ids.clone(), force.clone());
                    board.update(cx, |board, cx| board.remove_workspaces(ids, force, cx));
                },
            )
            .rows(rows)
            .open(window, cx);
        },
    );
}
