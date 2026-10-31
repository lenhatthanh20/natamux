//! Workflow test T7 (plan 7.5): a title an input method commits, then Enter, adds one task.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{AppContext as _, EntityInputHandler, Focusable, TestAppContext};

use super::AddTaskDialog;
use crate::app::test_driver::{press, text_of};
use crate::app::test_views::{self, Counted};
use crate::components::dialog::DialogLayer;

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
