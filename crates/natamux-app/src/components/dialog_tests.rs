//! Workflow test T6 (plan 7.5), the dialog part: Tab and Shift+Tab walk each dialog's ring in
//! order and wrap, with the focus staying inside the dialog.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::confirm::Confirm;
use super::dialog::DialogLayer;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::shortcut_sheet;
use crate::app::test_driver::press;
use crate::app::test_views::{self, Counted};
use crate::git::rollback_dialog;
use crate::projects::{issue_import, projects_dialog};
use crate::tasks::{add_subtask_dialog, add_task_dialog, fan_out_dialog};
use crate::workspaces::workspace_view::WorkspaceView;
use crate::workspaces::{new_workspace_dialog, remove_workspace_dialog};

type Opener = fn(&Entity<Dashboard>, &mut Window, &mut App);

fn task(dashboard: &Entity<Dashboard>, id: &str, cx: &App) -> Task {
    dashboard
        .read(cx)
        .data
        .task(id)
        .cloned()
        .expect("a task of the fake")
}

fn add_task(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    add_task_dialog::open(dashboard, None, window, cx);
}

fn add_subtask(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    add_subtask_dialog::init(dashboard, cx);
    let parent = task(dashboard, "T-0146", cx);
    add_subtask_dialog::open(parent, window, cx);
}

fn fan_out(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let task = task(dashboard, "T-0146", cx);
    fan_out_dialog::open(dashboard, &task, window, cx);
}

fn shortcuts(_: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    shortcut_sheet::open(window, cx);
}

fn projects(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    projects_dialog::open(dashboard, window, cx);
}

fn issues(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    issue_import::open(dashboard, None, window, cx);
}

fn remove_workspace(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let task = task(dashboard, "T-0139", cx);
    remove_workspace_dialog::open(dashboard, task, window, cx);
}

fn new_workspace(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    new_workspace_dialog::open(dashboard, None, window, cx);
}

fn confirm(_: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    Confirm::new("Delete T-0136?", "It goes.", "Delete", |_, _| {}).open(window, cx);
}

fn rollback(dashboard: &Entity<Dashboard>, window: &mut Window, cx: &mut App) {
    let workspace = cx.new(|cx| WorkspaceView::new(dashboard.clone(), cx));
    let files = fake_daemon::shared().list_changes("T-0143", false).unwrap();
    rollback_dialog::show(workspace.downgrade(), files, window, cx);
}

/// Each dialog the app opens from a key, a button or a row menu, by name. Send back has its own
/// tests (T10).
fn openers() -> [(&'static str, Opener); 10] {
    [
        ("add-task", add_task as Opener),
        ("add-subtask", add_subtask as Opener),
        ("fan-out", fan_out as Opener),
        ("shortcuts", shortcuts as Opener),
        ("projects", projects as Opener),
        ("issue-import", issues as Opener),
        ("remove-workspace", remove_workspace as Opener),
        ("new-workspace", new_workspace as Opener),
        ("confirm", confirm as Opener),
        ("rollback", rollback as Opener),
    ]
}

/// The place in the top dialog's ring that has the focus.
fn place(cx: &mut VisualTestContext) -> Option<usize> {
    cx.update(|window, cx| {
        let ring = DialogLayer::top_ring(cx);
        ring.iter().position(|h| h.contains_focused(window, cx))
    })
}

#[gpui::test]
fn t6_tab_and_shift_tab_walk_the_ring_of_each_dialog(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_app(cx, |_, _, cx| cx.new(|_| Counted { draws: 0 }));
    let mut wrong = Vec::new();
    for (name, open) in openers() {
        cx.update(|window, cx| open(&dashboard, window, cx));
        cx.run_until_parked();
        let len = cx.update(|_, cx| DialogLayer::top_ring(cx).len());
        let start = place(cx);
        // Issue import opens with the focus on the dialog itself, before its first stop.
        let outside = name == "issue-import";
        if len == 0 || start.is_none() != outside {
            wrong.push(format!("{name}: ring of {len}, focus at {start:?}"));
            continue;
        }
        // From outside the ring, Tab goes to the first stop.
        let start = start.unwrap_or(len - 1);
        let mut walked = Vec::new();
        for _ in 0..len {
            press(cx, "tab");
            walked.push(place(cx));
        }
        press(cx, "shift-tab");
        walked.push(place(cx));
        let want: Vec<_> = (1..=len)
            .map(|step| Some((start + step) % len))
            .chain([Some((start + len - 1) % len)])
            .collect();
        if walked != want {
            wrong.push(format!(
                "{name}: from {start} of {len}, got {walked:?}, want {want:?}"
            ));
        }
        if !cx.update(|_, cx| DialogLayer::is_open(cx)) {
            wrong.push(format!("{name}: closed"));
        }
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}
