//! The Remove finished workspaces confirm of a project (`ClearFinishedDialog.tsx`). The removal
//! runs one workspace after the other and ends with one summary toast
//! (`Dashboard::remove_workspaces`, rules in `remove_finished_rules.rs`).

use gpui::Entity;
use ui::prelude::*;

use super::remove_dialog_rules::{remove_finished_description, remove_finished_title};
use crate::app::dashboard::Dashboard;
use crate::components::confirm::Confirm;
use crate::sidebar::workspace_groups::{RepoRef, workspace_groups};

/// Asks before the finished workspaces of the project are removed: the ones its sidebar group
/// folds into "N finished". Does nothing when it has none.
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
    let rows = group
        .finished
        .iter()
        .map(|task| (task.id.clone().into(), task.title.clone().into()))
        .collect();
    let dashboard = dashboard.clone();
    Confirm::new(
        remove_finished_title(ids.len()),
        remove_finished_description(&group.repo.name),
        "Remove",
        move |_, cx| {
            let ids = ids.clone();
            dashboard.update(cx, |dashboard, cx| dashboard.remove_workspaces(ids, cx));
        },
    )
    .rows(rows)
    .open(window, cx);
}
