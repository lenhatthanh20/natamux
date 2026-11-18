//! Workflow test T23 (plan 7.5), the app's menus in its root view on the fake daemon: each menu
//! opens through the menu layer with the focus in it, Esc and Tab close it with the focus back
//! where it was and no call made, and Enter on the first entry runs it once. Other menus are
//! tested next to their views (the tree's in `tree_rename_tests`, Start a task's in
//! `new_workspace_dialog_tests`, the edit form's priority, the stage list's, the card's agent
//! picker, and the dialogs' menus of T10, T12, T16 and T17); the keys of every menu in
//! `components/menu_tests.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext, point, px};

use super::Root;
use super::flow_tests::{parts, root};
use crate::app::account_menu::open_account_menu;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::shortcuts::NewTask;
use crate::app::test_driver::{click_on, press};
use crate::app::title_bar::open_add_menu;
use crate::components::dialog::DialogLayer;
use crate::components::menu::MenuLayer;
use crate::stages::stage_fields_view::{Choice, open_choices};

type Open = fn(&Entity<Root>, &Entity<Dashboard>, &mut VisualTestContext);

fn table_row(_: &Entity<Root>, _: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    press(cx, "shift-f10");
}

fn more(root: &Entity<Root>, _: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    let details = parts(root, cx).details;
    details.update_in(cx, |panel, window, cx| panel.open_more_menu(window, cx));
}

fn account(_: &Entity<Root>, dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| open_account_menu(dashboard, point(px(300.), px(30.)), window, cx));
}

fn add(_: &Entity<Root>, _: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| open_add_menu(point(px(300.), px(30.)), window, cx));
}

fn choices(_: &Entity<Root>, _: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        let entries = vec![
            Choice::new("First", Some(true), |_, _| {}),
            Choice::new("Second", None, |_, _| {}),
        ];
        open_choices(entries, point(px(300.), px(200.)), window, cx)
    });
}

/// Each menu opened from the task list, which has the focus, by name.
fn menus() -> [(&'static str, Open); 5] {
    [
        ("task table row (Shift+F10)", table_row as Open),
        ("details more", more as Open),
        ("account", account as Open),
        ("add", add as Open),
        ("stage field choices", choices as Open),
    ]
}

/// The task list with the focus, as the opener of each menu.
fn list_focus(root: &Entity<Root>, cx: &mut VisualTestContext) -> FocusHandle {
    let tasks = parts(root, cx).tasks;
    let handle = tasks.read_with(cx, |tasks, cx| tasks.focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    handle
}

fn is_open(cx: &mut VisualTestContext) -> Option<bool> {
    let menu = cx.update(|_, cx| MenuLayer::open_menu(cx))?;
    Some(cx.update(|window, cx| menu.focus_handle(cx).contains_focused(window, cx)))
}

#[gpui::test]
fn t23_each_menu_opens_with_the_focus_and_esc_and_tab_close_it(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let mut wrong = Vec::new();
    for (name, open) in menus() {
        for key in ["escape", "tab"] {
            let opener = list_focus(&root, cx);
            open(&root, &dashboard, cx);
            cx.run_until_parked();
            let opened = is_open(cx);
            press(cx, key);
            let closed = is_open(cx).is_none();
            let back = cx.update(|window, _| opener.is_focused(window));
            if opened != Some(true) || !closed || !back {
                wrong.push(format!(
                    "{name} {key}: opened with focus {opened:?}, closed {closed}, focus back {back}"
                ));
            }
        }
    }
    let sent = fake_daemon::shared().sent();
    if !sent.is_empty() {
        wrong.push(format!("daemon calls {sent:?}"));
    }
    if cx.update(|_, cx| DialogLayer::is_open(cx)) {
        wrong.push("a dialog opened".into());
    }
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

fn add_task_open(cx: &mut VisualTestContext) -> bool {
    let top = cx.update(|_, cx| DialogLayer::top_view(cx));
    top.is_some_and(|view| {
        view.downcast::<crate::tasks::add_task_dialog::AddTaskDialog>()
            .is_ok()
    })
}

/// The add menu, which has no toggled entry, opens with its first entry picked. Today Enter on
/// "Add a task" only closes the menu (see "Bugs found" of the MIT move): the entry sends the
/// app's action, and an action sent while the menu has the focus does not reach the root view.
/// The same action from the list, or a click on the entry, opens Add a task.
#[gpui::test]
fn t23_enter_in_the_add_menu_as_it_opens(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let opener = list_focus(&root, cx);
    add(&root, &dashboard, cx);
    cx.run_until_parked();
    let menu = cx.update(|_, cx| MenuLayer::open_menu(cx)).expect("open");
    let picked = menu.read_with(cx, |menu, _| menu.selected_index());
    press(cx, "enter");
    let from_menu = add_task_open(cx);
    let menu_open = is_open(cx).is_some();
    let list_focused = cx.update(|window, _| opener.is_focused(window));
    cx.update(|window, cx| window.dispatch_action(Box::new(NewTask), cx));
    cx.run_until_parked();
    let from_list = add_task_open(cx);
    press(cx, "escape");
    // The same action sent while the menu has the focus, with no Enter.
    list_focus(&root, cx);
    add(&root, &dashboard, cx);
    cx.run_until_parked();
    cx.update(|window, cx| window.dispatch_action(Box::new(NewTask), cx));
    cx.run_until_parked();
    let from_open_menu = add_task_open(cx);
    press(cx, "escape");
    // A click on the entry.
    list_focus(&root, cx);
    add(&root, &dashboard, cx);
    cx.run_until_parked();
    click_on(cx, "MENU_ITEM-Add a task");
    cx.run_until_parked();
    let from_click = add_task_open(cx);
    let seen = format!(
        "picked on open {picked:?}; enter: menu open {menu_open}, add a task {from_menu}, list \
         focused {list_focused}; the action from the list: add a task {from_list}; from the open \
         menu: add a task {from_open_menu}; a click: add a task {from_click}"
    );
    assert_eq!(
        seen,
        "picked on open Some(0); enter: menu open false, add a task false, list focused true; \
         the action from the list: add a task true; from the open menu: add a task false; a click: add a task \
         true"
    );
}
