//! `--shot STATE`: opens the window in one screen-1 state of the fake daemon, for the CI
//! screenshots (docs/rust-phase3.md). The clock stops at the fake's time and the view state is
//! fixed and never saved, so a shot is the same on every run.

use std::time::Duration;

use gpui::{App, AsyncApp, Focusable, Window, WindowHandle};

use super::clock;
use super::fake_daemon::{self, FakeLink, NOW, PINNED};
use super::options::Shot;
use super::shortcuts::OpenRowMenu;
use super::view_state::{Layout, ViewState};
use super::window::Root;
use crate::tasks::add_subtask_dialog;
use crate::tasks::task_time::parse_iso_ms;
use crate::workspaces::{new_workspace_dialog, remove_finished, remove_workspace_dialog};

/// The feed and the first frames come well within this.
const SETTLE: Duration = Duration::from_millis(800);
/// A toast lasts 5 s; it is shown again before it goes, so the shot always has it.
const TOAST_AGAIN: Duration = Duration::from_secs(3);
/// The task of the row menu, Remove workspace and Add subtask shots.
const TASK: &str = "T-0146";
/// The task whose new workspace is ready in the toast shot.
const READY: &str = "T-0144";
/// A task with a live terminal and no files to commit, for Remove workspace.
const REMOVED: &str = "T-0139";
const TYPED: &str = "Show the PR checks on the board cards";

/// Sets the fake's data before the window watches it. Call it before `window::open`.
pub fn prepare(shot: Shot) {
    if let Some(now) = parse_iso_ms(NOW) {
        clock::stop_at(now);
    }
    let (link, tasks, repos) = match shot {
        Shot::Skeleton => (FakeLink::Stalled, true, true),
        Shot::LoadError => (FakeLink::Down, true, true),
        Shot::Empty => (FakeLink::Live, false, true),
        Shot::NoProjects => (FakeLink::Live, false, false),
        _ => (FakeLink::Live, true, true),
    };
    fake_daemon::shared().set_up(link, tasks, repos);
}

pub fn run(handle: WindowHandle<Root>, shot: Shot, cx: &mut App) {
    let fixed = handle.update(cx, |root, _, cx| {
        root.dashboard()
            .update(cx, |dashboard, cx| dashboard.use_view(view_of(shot), cx))
    });
    if fixed.is_err() {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SETTLE).await;
        if let Err(error) = handle.update(cx, |root, window, cx| enter(shot, root, window, cx)) {
            eprintln!("shot: the window is gone: {error}");
            return;
        }
        if shot == Shot::Toast {
            keep_toast(handle, cx).await;
        }
    })
    .detach();
}

fn view_of(shot: Shot) -> ViewState {
    let board = matches!(shot, Shot::TasksBoard | Shot::BoardHoverCard);
    ViewState {
        pinned: PINNED.iter().map(|id| id.to_string()).collect(),
        layout: if board { Layout::Board } else { Layout::Table },
        ..ViewState::default()
    }
}

/// Puts the window in the state. The data is there: the fake's feed sends it at once.
fn enter(shot: Shot, root: &mut Root, window: &mut Window, cx: &mut gpui::Context<Root>) {
    let dashboard = root.dashboard().clone();
    let task = |id: &str, cx: &App| dashboard.read(cx).data.task(id).cloned();
    match shot {
        Shot::TasksTable | Shot::TasksBoard => select(root, TASK, window, cx),
        // A real pointer move can be undone by the system before the capture, so the board
        // draws the hover look itself.
        Shot::BoardHoverCard => {
            let board = root.task_view().read(cx).board().clone();
            let first = board.read(cx).first_card(cx);
            board.update(cx, |board, cx| board.show_hover(first, cx));
        }
        Shot::AddTaskTyped => {
            let quick_add = root.task_view().read(cx).quick_add().clone();
            quick_add.update(cx, |quick_add, cx| {
                quick_add.set_title(TYPED, window, cx);
                quick_add.focus(window, cx);
            });
        }
        Shot::StartTask => new_workspace_dialog::open(&dashboard, None, window, cx),
        Shot::RemoveWorkspace => {
            if let Some(task) = task(REMOVED, cx) {
                remove_workspace_dialog::open(&dashboard, task, window, cx);
            }
        }
        Shot::ClearFinished => {
            let repo = dashboard
                .read(cx)
                .data
                .repos
                .first()
                .map(|repo| repo.id.clone());
            if let Some(repo) = repo {
                remove_finished::open(&dashboard, repo, window, cx);
            }
        }
        Shot::AddSubtask => {
            if let Some(task) = task(TASK, cx) {
                add_subtask_dialog::open(task, window, cx);
            }
        }
        // The sidebar row's menu (ui-spec 4.13). Both shot sizes are under 1024, so the sidebar is
        // the drawer. Each step waits a frame: the drawer lays the tree out, then the focused row.
        Shot::TaskContextMenu => {
            root.set_drawer(true, window, cx);
            let tree = root.sidebar().read(cx).tree().clone();
            window.on_next_frame(move |window, cx| {
                tree.update(cx, |tree, cx| tree.focus_task(TASK, window, cx));
                window
                    .on_next_frame(|window, cx| window.dispatch_action(Box::new(OpenRowMenu), cx));
            });
        }
        Shot::FilterMiss => {
            let filter = root.filter(cx);
            filter.update(cx, |editor, cx| editor.set_text("zebra", window, cx));
        }
        Shot::Drawer => root.set_drawer(true, window, cx),
        Shot::Toast | Shot::Skeleton | Shot::Empty | Shot::NoProjects | Shot::LoadError => {}
    }
}

/// Selects the task in the table and gives the table the focus, as a click does.
fn select(root: &mut Root, id: &str, window: &mut Window, cx: &mut gpui::Context<Root>) {
    root.dashboard().update(cx, |dashboard, cx| {
        dashboard.select(Some(id.to_string()), cx)
    });
    let table = root.task_view().read(cx).table().clone();
    let focus = table.read(cx).focus_handle(cx);
    window.focus(&focus, cx);
}

/// The toast of a new workspace that is ready, made by the app's own rule (`onPrepared`). The
/// same text again replaces the toast, so one shows.
async fn keep_toast(handle: WindowHandle<Root>, cx: &mut AsyncApp) {
    loop {
        let shown = handle.update(cx, |root, _, cx| {
            root.dashboard().update(cx, |dashboard, cx| {
                let ready = dashboard.data.prepared(READY, None);
                dashboard.toast(&ready.text, ready.tone, None, cx);
            })
        });
        if shown.is_err() {
            return;
        }
        cx.background_executor().timer(TOAST_AGAIN).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_board_shots_open_the_board() {
        for shot in Shot::ALL {
            let board = matches!(shot, Shot::TasksBoard | Shot::BoardHoverCard);
            let layout = if board { Layout::Board } else { Layout::Table };
            assert_eq!(view_of(shot).layout, layout, "{}", shot.name());
            assert_eq!(view_of(shot).pinned, vec!["T-0141".to_string()]);
        }
    }

    #[test]
    fn the_shots_name_tasks_the_fake_has() {
        let fake = fake_daemon::FakeDaemon::default();
        for id in [TASK, REMOVED] {
            assert!(fake.list_events(id).is_ok());
            assert!(fake.graph(id).is_ok());
        }
        assert!(fake.list_terminals(REMOVED).is_ok());
        assert!(fake.list_changes(REMOVED).unwrap().is_empty());
    }
}
