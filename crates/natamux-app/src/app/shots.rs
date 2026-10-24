//! `--shot STATE`: opens the window in one screen-1 to 6 state of the fake daemon, for the CI
//! screenshots (docs/rust-phase3.md). The clock stops at the fake's time and the view state is
//! fixed and never saved, so a shot is the same on every run.

use std::time::Duration;

use gpui::{App, AsyncApp, Focusable, Window, WindowHandle};
use natamux_proto::workspaces::workspace::PreparingStep;

use super::clock;
use super::dashboard::Dashboard;
use super::fake_daemon::{self, FakeLink, NOW, PINNED};
use super::options::Shot;
use super::shortcuts::{FindInTerminal, OpenRowMenu};
use super::shots_files;
use super::shots_settings;
use super::shots_stages;
use super::view_rules::{DetailsView, WorkspaceTab};
use super::view_state::{Layout, ViewState};
use super::window::Root;
use crate::tasks::task_time::parse_iso_ms;
use crate::tasks::{add_subtask_dialog, fan_out_dialog};
use crate::terminals::terminal_cache::{TerminalCache, terminal_key};
use crate::workspaces::{new_workspace_dialog, remove_finished, remove_workspace_dialog};

/// The feed and the first frames come well within this.
const SETTLE: Duration = Duration::from_millis(800);
/// Zed's terminal anchors its text to the bottom by the screen it read at its last draw, so a
/// screen that comes in after the last draw sat half a line higher in some runs (run
/// 40022329180). The window draws again this often, for as long as CI takes to shoot it.
const REDRAW_EVERY: Duration = Duration::from_millis(200);
const REDRAWS: usize = 20;
/// A toast lasts 5 s; it is shown again before it goes, so the shot always has it.
const TOAST_AGAIN: Duration = Duration::from_secs(3);
/// The task of the row menu, Remove workspace and Add subtask shots.
const TASK: &str = "T-0146";
/// The task whose new workspace is ready in the toast shot.
const READY: &str = "T-0144";
/// A task with a live terminal and no files to commit, for Remove workspace.
const REMOVED: &str = "T-0139";
const TYPED: &str = "Show the PR checks on the board cards";
/// Screen 2: a side by side group at work, an agent that asks, a task after review, a task with
/// no workspace, and a done one.
const RUNNING: &str = "T-0146";
const ASKING: &str = "T-0141";
const REVIEWED: &str = "T-0143";
const NO_WORKSPACE: &str = "T-0135";
const FINISHED: &str = "T-0139";
/// The find shot's term: Terminal 1 of the reviewed task shows it five times.
const FOUND: &str = "src";
/// The terminals draw their first screen within this.
const PANES_DRAWN: Duration = Duration::from_millis(1200);
/// Screen 3: a plan that waits for approval, a failed plan, and the second of three tries.
const PLAN_GATE: &str = "T-0150";
const PLAN_FAILED: &str = "T-0151";
const A_TRY: &str = "T-0152";
const SEND_BACK_REASON: &str = "Add the error cases for an empty list.";
/// The Tries section is the sixth part of the Details tab of a try with an agent.
const TRIES_PART: usize = 5;
/// Screen 4: a running task with no file to commit, one with a new image; the reviewed task has
/// two changed files.
const CLEAN: &str = "T-0144";
const IMAGE: &str = "T-0147";
const CHANGED_FILE: &str = "src/checkout.ts";
/// The lines Ask agent quotes: the new body of the function.
const ASKED_LINES: (u32, u32) = (5, 6);

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
    if shot == Shot::WorkspacePreparing {
        fake_daemon::shared().set_preparing(NO_WORKSPACE, PreparingStep::Fetch);
    }
    shots_settings::prepare(shot);
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
        for _ in 0..REDRAWS {
            cx.background_executor().timer(REDRAW_EVERY).await;
            if handle.update(cx, |_, window, _| window.refresh()).is_err() {
                return;
            }
        }
    })
    .detach();
    cx.spawn(async move |cx| {
        cx.background_executor().timer(SETTLE).await;
        if shots_stages::is_settings_shot(shot) {
            shots_stages::run(handle, shot, cx).await;
            return;
        }
        if shots_files::is_files_shot(shot) {
            shots_files::run(handle, shot, cx).await;
            return;
        }
        if shots_settings::is_screen7_shot(shot) {
            shots_settings::run(handle, shot, cx).await;
            return;
        }
        if let Err(error) = handle.update(cx, |root, window, cx| enter(shot, root, window, cx)) {
            eprintln!("shot: the window is gone: {error}");
            return;
        }
        if shot == Shot::Toast {
            keep_toast(handle, cx).await;
        }
        if matches!(
            shot,
            Shot::SendBack
                | Shot::DetailsMoreMenu
                | Shot::CompareTries
                | Shot::WorkspaceChangesDiff
                | Shot::WorkspaceChangesAsk
                | Shot::Rollback
        ) {
            cx.background_executor().timer(SETTLE).await;
            handle
                .update(cx, |root, window, cx| panel_step(shot, root, window, cx))
                .ok();
        }
        if shot == Shot::TerminalFind {
            cx.background_executor().timer(PANES_DRAWN).await;
            handle
                .update(cx, |_, window, cx| find_in_pane(REVIEWED, window, cx))
                .ok();
        }
    })
    .detach();
}

fn view_of(shot: Shot) -> ViewState {
    let board = matches!(shot, Shot::TasksBoard | Shot::BoardHoverCard);
    let agents = (shot == Shot::AgentsGrid).then(|| vec![ASKING.to_string(), RUNNING.to_string()]);
    let mut view = ViewState {
        pinned: PINNED.iter().map(|id| id.to_string()).collect(),
        layout: match shot {
            _ if board => Layout::Board,
            Shot::TasksProgress => Layout::Progress,
            _ => Layout::Table,
        },
        agent_panes: agents,
        ..ViewState::default()
    };
    // The palette lists the workspaces opened last.
    view.recent = vec![RUNNING.to_string(), REVIEWED.to_string()];
    // The Ask agent shot shows the inline diff; the others keep side by side.
    view.diff.side_by_side = shot != Shot::WorkspaceChangesAsk;
    view
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
        Shot::TaskContextMenu => row_menu(root, window, cx),
        Shot::FilterMiss => {
            let filter = root.filter(cx);
            filter.update(cx, |editor, cx| editor.set_text("zebra", window, cx));
        }
        Shot::Drawer => root.set_drawer(true, window, cx),
        Shot::WorkspaceTerminalRunning => open(&dashboard, RUNNING, cx),
        Shot::TerminalFind | Shot::WorkspaceTerminalReview => open(&dashboard, REVIEWED, cx),
        Shot::WorkspaceTerminalNeedsInput => open(&dashboard, ASKING, cx),
        Shot::NoWorkspace | Shot::WorkspacePreparing => open(&dashboard, NO_WORKSPACE, cx),
        Shot::FinishedWrapUp => open(&dashboard, FINISHED, cx),
        Shot::AgentsGrid => dashboard.update(cx, |dashboard, cx| dashboard.open_agents(cx)),
        Shot::TaskDetailsReview => choose(root, REVIEWED, window, cx),
        Shot::TaskDetailsNeedsInput => choose(root, ASKING, window, cx),
        Shot::AnswerBanner => progress(&dashboard, ASKING, cx),
        Shot::WorkspaceTerminalPlanGate => progress(&dashboard, PLAN_GATE, cx),
        Shot::WorkspaceProgress | Shot::SendBack => progress(&dashboard, REVIEWED, cx),
        Shot::StageFailed => progress(&dashboard, PLAN_FAILED, cx),
        Shot::DetailsDetails | Shot::DetailsMoreMenu => details(&dashboard, REVIEWED, cx),
        Shot::CompareTries => details(&dashboard, A_TRY, cx),
        Shot::FanOut => {
            details(&dashboard, NO_WORKSPACE, cx);
            if let Some(task) = task(NO_WORKSPACE, cx) {
                fan_out_dialog::open(&dashboard, &task, window, cx);
            }
        }
        // Screen 4, and the states that need no step.
        _ => enter_git(shot, &dashboard, cx),
    }
}

/// Screen 4: the workspace of the state's task on Changes or History.
fn enter_git(shot: Shot, dashboard: &gpui::Entity<Dashboard>, cx: &mut App) {
    match shot {
        Shot::WorkspaceChangesEmpty => changes(dashboard, CLEAN, cx),
        Shot::WorkspaceChangesList
        | Shot::WorkspaceChangesDiff
        | Shot::WorkspaceChangesAsk
        | Shot::Rollback => changes(dashboard, REVIEWED, cx),
        Shot::ImageDiff => changes(dashboard, IMAGE, cx),
        Shot::WorkspaceHistory => dashboard.update(cx, |dashboard, cx| {
            dashboard.open(REVIEWED, cx);
            dashboard.pick_tab(WorkspaceTab::History, cx);
        }),
        _ => {}
    }
}

/// The drawer, then the task's row focused, then its menu, each a frame after the other.
fn row_menu(root: &mut Root, window: &mut Window, cx: &mut gpui::Context<Root>) {
    root.set_drawer(true, window, cx);
    let tree = root.sidebar().read(cx).tree().clone();
    window.on_next_frame(move |window, cx| {
        tree.update(cx, |tree, cx| tree.focus_task(TASK, window, cx));
        window.on_next_frame(|window, cx| window.dispatch_action(Box::new(OpenRowMenu), cx));
    });
}

/// The task's workspace on its Changes tab.
fn changes(dashboard: &gpui::Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| dashboard.open_changes(id, false, cx));
}

fn open(dashboard: &gpui::Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| dashboard.open(id, cx));
}

/// The task's workspace with its panel on Progress.
fn progress(dashboard: &gpui::Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(id, cx);
        dashboard.show_progress(id, cx);
    });
}

/// The task's workspace with its panel on Details.
fn details(dashboard: &gpui::Entity<Dashboard>, id: &str, cx: &mut App) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(id, cx);
        dashboard.set_details_view(DetailsView::Details, cx);
        dashboard.set_details_open(id, true, cx);
    });
}

/// A table row clicked: selected, with its panel open on Details.
fn choose(root: &mut Root, id: &str, window: &mut Window, cx: &mut gpui::Context<Root>) {
    select(root, id, window, cx);
    root.dashboard()
        .update(cx, |dashboard, cx| dashboard.choose(id, cx));
}

/// What needs the panel or the diff drawn first: the Send back dialog, the "..." menu, the
/// Tries section, a change of the diff, Ask agent, Rollback.
fn panel_step(shot: Shot, root: &mut Root, window: &mut Window, cx: &mut gpui::Context<Root>) {
    let panel = root.details_panel().clone();
    match shot {
        Shot::SendBack => {
            let dialog = panel.update(cx, |panel, cx| panel.send_back_waiting(window, cx));
            if let Some(dialog) = dialog {
                dialog.update(cx, |dialog, cx| {
                    dialog.set_reason(SEND_BACK_REASON, window, cx)
                });
            }
        }
        Shot::DetailsMoreMenu => panel.update(cx, |panel, cx| panel.open_more_menu(window, cx)),
        Shot::CompareTries => panel.update(cx, |panel, cx| panel.center_on(TRIES_PART, cx)),
        Shot::WorkspaceChangesDiff | Shot::WorkspaceChangesAsk | Shot::Rollback => {
            let changes = root.workspace_view().read(cx).changes_view().clone();
            changes.update(cx, |changes, cx| match shot {
                Shot::WorkspaceChangesDiff => changes.shot_change(CHANGED_FILE, 2, window, cx),
                Shot::WorkspaceChangesAsk => {
                    changes.shot_ask(ASKED_LINES.0, ASKED_LINES.1, window, cx)
                }
                _ => changes.shot_rollback(window, cx),
            });
        }
        _ => {}
    }
}

/// Mod+F in Terminal 1 of the task, and the term typed.
fn find_in_pane(task_id: &str, window: &mut Window, cx: &mut App) {
    let key = terminal_key(task_id, "1");
    let Some(terminal) = TerminalCache::cached(&key, cx) else {
        eprintln!("shot: Terminal 1 of {task_id} is not on screen");
        return;
    };
    let focus = terminal.read(cx).view.focus_handle(cx);
    window.focus(&focus, cx);
    window.dispatch_action(Box::new(FindInTerminal), cx);
    let term = FOUND.to_string();
    window.on_next_frame(move |window, cx| {
        if let Some(find) = TerminalCache::find(&key, cx) {
            find.update(cx, |find, cx| find.set_term(&term, window, cx));
        }
    });
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
    fn only_the_board_and_progress_shots_leave_the_table() {
        for shot in Shot::ALL {
            let layout = match shot {
                Shot::TasksBoard | Shot::BoardHoverCard => Layout::Board,
                Shot::TasksProgress => Layout::Progress,
                _ => Layout::Table,
            };
            assert_eq!(view_of(shot).layout, layout, "{}", shot.name());
            assert_eq!(view_of(shot).pinned, vec!["T-0141".to_string()]);
        }
    }

    #[test]
    fn the_shots_name_tasks_the_fake_has() {
        let fake = fake_daemon::FakeDaemon::default();
        for id in [TASK, REMOVED] {
            assert!(fake.list_events(id).is_ok());
            assert!(fake.graph(id, false).is_ok());
        }
        for id in [RUNNING, ASKING, REVIEWED, PLAN_GATE, PLAN_FAILED] {
            assert!(fake.list_terminals(id).is_ok(), "{id}");
            assert!(fake.graph(id, false).is_ok(), "{id}");
        }
        let tasks = fake.tasks();
        let task = |id: &str| tasks.iter().find(|task| task.id == id).cloned().unwrap();
        assert!(task(NO_WORKSPACE).workspace.is_none());
        assert!(
            task(A_TRY)
                .labels
                .iter()
                .any(|label| label == "tries-t-0135")
        );
        assert!(task(FINISHED).workspace.is_some());
        assert_eq!(fake.ended_shells(FINISHED).unwrap().len(), 5);
        assert!(fake.list_terminals(REMOVED).is_ok());
        assert!(fake.list_changes(REMOVED, false).unwrap().is_empty());
        assert!(fake.list_changes(CLEAN, false).unwrap().is_empty());
        let reviewed = fake.list_changes(REVIEWED, false).unwrap();
        assert!(reviewed.iter().any(|entry| entry.path == CHANGED_FILE));
        assert!(fake.file_diff(REVIEWED, CHANGED_FILE).is_ok());
        assert!(!fake.graph(REVIEWED, false).unwrap().commits.is_empty());
        let image = fake.list_changes(IMAGE, false).unwrap();
        assert!(image.iter().any(|entry| entry.path.ends_with(".png")));
    }
}
