//! Workflow test T18 (plan 7.5): Enter and Mod+Enter add the subtask once, the task chosen before
//! stays chosen, and an error keeps the dialog with the title.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, TestAppContext, VisualTestContext};
use natamux_proto::tasks::task::Task;

use super::{AddSubtaskDialog, init, open};
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::{is_focused, press, text_of, type_text};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;

const PARENT: &str = "T-0146";
/// Chosen before the dialog opens.
const CHOSEN: &str = "T-0143";

fn open_dialog(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Entity<AddSubtaskDialog> {
    dashboard.update(cx, |dashboard, cx| dashboard.select(Some(CHOSEN.into()), cx));
    cx.update(|window, cx| {
        init(dashboard, cx);
        let parent = dashboard.read(cx).data.task(PARENT).cloned().expect("a task");
        open(parent, window, cx);
    });
    cx.run_until_parked();
    let view = cx.update(|_, cx| DialogLayer::top_view(cx)).expect("open");
    view.downcast::<AddSubtaskDialog>().ok().expect("the add subtask dialog")
}

fn children(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Vec<Task> {
    dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        tasks
            .filter(|task| task.parent_id.as_deref() == Some(PARENT))
            .cloned()
            .collect()
    })
}

fn selected(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Option<String> {
    dashboard.read_with(cx, |dashboard, _| dashboard.selected.clone())
}

fn adds_once_with(keys: &str, cx: &mut TestAppContext) {
    let (opener, dashboard, cx) = test_views::init_opener(cx);
    let before = children(&dashboard, cx).len();
    open_dialog(&dashboard, cx);
    type_text(cx, "Write the tests");
    // The second press comes while the first one waits for the daemon.
    press(cx, &format!("{keys} {keys}"));
    let added: Vec<String> = children(&dashboard, cx)
        .into_iter()
        .filter(|task| task.title == "Write the tests")
        .map(|task| task.id)
        .collect();
    assert_eq!(added.len(), 1, "{keys}: {added:?}");
    assert_eq!(children(&dashboard, cx).len(), before + 1);
    assert_eq!(selected(&dashboard, cx).as_deref(), Some(CHOSEN), "{keys}");
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)), "{keys}");
    assert!(is_focused(&opener, cx), "{keys}");
}

#[gpui::test]
fn t18_enter_adds_once_and_keeps_the_chosen_task(cx: &mut TestAppContext) {
    adds_once_with("enter", cx);
}

#[gpui::test]
fn t18_mod_enter_adds_once_and_keeps_the_chosen_task(cx: &mut TestAppContext) {
    adds_once_with("mod-enter", cx);
}

#[gpui::test]
fn t18_an_error_keeps_the_dialog_and_the_title(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let before = children(&dashboard, cx).len();
    let dialog = open_dialog(&dashboard, cx);
    type_text(cx, "Write the tests");
    fake_daemon::shared().fail_next("create_task");
    press(cx, "enter");
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
    let (busy, title) = dialog.read_with(cx, |dialog, _| (dialog.busy, dialog.title.clone()));
    assert!(!busy);
    assert_eq!(text_of(&title, cx), "Write the tests");
    assert_eq!(children(&dashboard, cx).len(), before);
    assert_eq!(selected(&dashboard, cx).as_deref(), Some(CHOSEN));
    // The next try goes through.
    press(cx, "enter");
    assert_eq!(children(&dashboard, cx).len(), before + 1);
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)));
}

#[gpui::test]
fn t18_an_empty_title_adds_nothing(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let before = children(&dashboard, cx).len();
    open_dialog(&dashboard, cx);
    type_text(cx, "   ");
    press(cx, "enter mod-enter");
    assert_eq!(children(&dashboard, cx).len(), before);
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
}
