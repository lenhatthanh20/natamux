//! Workflow tests T7 and T17 (plan 7.5): a title an input method commits, then Enter, adds one
//! task; Enter adds once; Mod+Enter and the Start a task button open Start a task with the title;
//! the project and template menus pick what Enter adds.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{
    AppContext as _, Entity, EntityInputHandler, Focusable, TestAppContext, VisualTestContext,
};
use natamux_proto::tasks::task::Task;

use super::AddTaskDialog;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::{activate, click_on, press, text_of, type_text};
use crate::app::test_views::{self, Counted};
use crate::components::dialog::DialogLayer;
use crate::workspaces::new_workspace_dialog::NewWorkspaceDialog;

#[gpui::test]
fn t7_an_ime_commit_then_enter_adds_one_task(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_app(cx, |_, _, cx| cx.new(|_| Counted { draws: 0 }));
    let before = dashboard.read_with(cx, |dashboard, _| dashboard.data.tasks.len());
    // As `open` does, keeping the dialog to reach its title box.
    let dialog = cx.update(|window, cx| {
        let dialog = cx.new(|cx| AddTaskDialog::new(dashboard.clone(), None, window, cx));
        let field = dialog.read(cx).title.focus_handle(cx);
        DialogLayer::open(dialog.clone(), window, cx);
        window.focus(&field, cx);
        dialog
    });
    cx.run_until_parked();
    let title = dialog.read_with(cx, |dialog, _| dialog.title.clone());
    // Telex: the marked steps of "việt", then the commit.
    title.update_in(cx, |title, window, cx| {
        for step in ["v", "vi", "vie", "viê", "việ"] {
            title.replace_and_mark_text_in_range(None, step, None, window, cx);
        }
        title.replace_text_in_range(None, "việt", window, cx);
    });
    cx.run_until_parked();
    assert_eq!(text_of(&title, cx), "việt");

    press(cx, "enter");
    let added: Vec<String> = dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        tasks
            .filter(|task| task.title == "việt")
            .map(|task| task.id.clone())
            .collect()
    });
    assert_eq!(added.len(), 1, "{added:?}");
    let after = dashboard.read_with(cx, |dashboard, _| dashboard.data.tasks.len());
    assert_eq!(after, before + 1);
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)));
}

/// Opens the dialog as `n` does, and gives it.
fn open_dialog(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Entity<AddTaskDialog> {
    cx.update(|window, cx| super::open(dashboard, None, window, cx));
    cx.run_until_parked();
    let view = cx.update(|_, cx| DialogLayer::top_view(cx)).expect("open");
    view.downcast::<AddTaskDialog>()
        .expect("the add task dialog")
}

fn titled(dashboard: &Entity<Dashboard>, title: &str, cx: &mut VisualTestContext) -> Vec<Task> {
    dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        tasks.filter(|task| task.title == title).cloned().collect()
    })
}

/// The title of the Start a task dialog on top, if that is the dialog on top.
fn start_title(cx: &mut VisualTestContext) -> Option<String> {
    let view = cx.update(|_, cx| DialogLayer::top_view(cx))?;
    let dialog = view.downcast::<NewWorkspaceDialog>().ok()?;
    let title = dialog.read_with(cx, |dialog, _| dialog.title_box().clone());
    Some(text_of(&title, cx))
}

#[gpui::test]
fn t17_enter_adds_once(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    open_dialog(&dashboard, cx);
    type_text(cx, "Fix it");
    // The second Enter comes while the first one waits for the daemon.
    press(cx, "enter enter");
    assert_eq!(titled(&dashboard, "Fix it", cx).len(), 1);
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)));
}

#[gpui::test]
fn t17_mod_enter_opens_start_a_task_with_the_title(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    open_dialog(&dashboard, cx);
    type_text(cx, "Fix it");
    press(cx, "mod-enter");
    assert_eq!(start_title(cx).as_deref(), Some("Fix it"));
    assert!(titled(&dashboard, "Fix it", cx).is_empty());
}

#[gpui::test]
fn t17_the_start_button_opens_start_a_task_with_the_title(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open_dialog(&dashboard, cx);
    type_text(cx, "Fix it");
    let start = dialog.read_with(cx, |dialog, _| dialog.start_focus.clone());
    cx.update(|window, cx| window.focus(&start, cx));
    cx.run_until_parked();
    activate(cx);
    assert_eq!(start_title(cx).as_deref(), Some("Fix it"));
    assert!(titled(&dashboard, "Fix it", cx).is_empty());
}

#[gpui::test]
fn t17_the_project_and_template_menus_pick_what_enter_adds(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open_dialog(&dashboard, cx);
    // Title, Project, Template, Start a task, Add: two projects and two templates.
    assert_eq!(cx.update(|_, cx| DialogLayer::top_ring(cx).len()), 5);
    press(cx, "tab");
    activate(cx);
    click_on(cx, "MENU_ITEM-natamux-site");
    press(cx, "tab");
    activate(cx);
    click_on(cx, "MENU_ITEM-Bug report");
    let picked = dialog.read_with(cx, |dialog, _| {
        (dialog.repo.clone(), dialog.template.clone())
    });
    assert_eq!(
        picked,
        (Some("natamux-site".to_string()), Some("bug".to_string()))
    );
    press(cx, "shift-tab shift-tab");
    type_text(cx, "Crash");
    press(cx, "enter");
    let added = titled(&dashboard, "Crash", cx);
    assert_eq!(added.len(), 1, "{added:?}");
    assert_eq!(added[0].repo.as_deref(), Some("natamux-site"));
    let template = fake_daemon::shared().list_templates().unwrap();
    let bug = template.iter().find(|t| t.id == "bug").unwrap();
    assert_eq!(added[0].labels, bug.labels);
}
