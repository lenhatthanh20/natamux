//! Workflow test T16 (plan 7.5): Run more tries opens with 2 tries and Start on, Mod+Enter makes
//! the tries in one call, and Esc and Cancel make none.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::FanOutDialog;
use crate::app::dashboard::Dashboard;
use crate::app::test_driver::{activate, click_on, is_focused, press};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;

/// Running in the fake, with no tries yet.
const TASK: &str = "T-0146";

/// The tasks of the try group of `TASK`, the source included once it joined.
fn tries(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Vec<String> {
    let group = format!("tries-{}", TASK.to_lowercase());
    dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        tasks
            .filter(|task| task.id != TASK && task.labels.contains(&group))
            .map(|task| task.title.clone())
            .collect()
    })
}

/// Opens the dialog as the details panel does, and gives it.
fn open(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Entity<FanOutDialog> {
    cx.update(|window, cx| {
        let task = dashboard.read(cx).data.task(TASK).cloned().expect("a task");
        super::open(dashboard, &task, window, cx);
    });
    cx.run_until_parked();
    let view = cx.update(|_, cx| DialogLayer::top_view(cx)).expect("open");
    view.downcast::<FanOutDialog>().ok().expect("the fan out dialog")
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| DialogLayer::is_open(cx))
}

#[gpui::test]
fn t16_opens_with_two_tries_and_start_on_and_the_focus_on_the_count(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open(&dashboard, cx);
    let (count, start, first) = dialog.read_with(cx, |dialog, _| {
        (dialog.count, dialog.start, dialog.count_focus.clone())
    });
    assert_eq!((count, start), (2, true));
    assert!(cx.update(|window, _| first.is_focused(window)));
}

#[gpui::test]
fn t16_mod_enter_makes_the_tries_in_one_call(cx: &mut TestAppContext) {
    let (opener, dashboard, cx) = test_views::init_opener(cx);
    open(&dashboard, cx);
    press(cx, "mod-enter");
    let made = tries(&dashboard, cx);
    let title = "Rust migration phase 3";
    assert_eq!(
        made,
        [format!("{title} (try 2)"), format!("{title} (try 3)")],
        "{made:?}"
    );
    assert!(!is_open(cx));
    assert!(is_focused(&opener, cx));
}

#[gpui::test]
fn t16_the_count_menu_and_the_start_box_change_the_call(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open(&dashboard, cx);
    activate(cx);
    click_on(cx, "MENU_ITEM-3");
    press(cx, "tab");
    activate(cx);
    let (count, start) = dialog.read_with(cx, |dialog, _| (dialog.count, dialog.start));
    assert_eq!((count, start), (3, false));
    press(cx, "mod-enter");
    assert_eq!(tries(&dashboard, cx).len(), 3);
}

#[gpui::test]
fn t16_esc_and_cancel_make_no_tries(cx: &mut TestAppContext) {
    let (opener, dashboard, cx) = test_views::init_opener(cx);
    open(&dashboard, cx);
    press(cx, "escape");
    assert!(!is_open(cx));
    assert!(is_focused(&opener, cx));

    open(&dashboard, cx);
    // Count, Start, then Cancel.
    press(cx, "tab tab");
    activate(cx);
    assert!(!is_open(cx));
    assert!(is_focused(&opener, cx));
    assert!(tries(&dashboard, cx).is_empty());
}
