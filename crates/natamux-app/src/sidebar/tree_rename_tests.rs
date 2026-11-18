//! Workflow test T27 (plan 7.5), in the app's root view on the fake daemon: the sidebar's inline
//! rename saves on Enter and on a blur, drops on Esc, and sends nothing for an unchanged title.
//! Plus the tree's two menus (T23): Shift+F10 opens them on the focused row, and Esc closes them
//! with the focus back on the tree.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext};

use super::super::workspace_tree::WorkspaceTree;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::{is_focused, press, type_text};
use crate::app::test_views::PROJECT;
use crate::app::window::flow_tests::{REVIEWED, parts, root};
use crate::components::menu::MenuLayer;

fn title(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> String {
    dashboard.read_with(cx, |dashboard, _| {
        dashboard.data.task(REVIEWED).expect("a task").title.clone()
    })
}

fn renaming(tree: &Entity<WorkspaceTree>, cx: &mut VisualTestContext) -> bool {
    tree.read_with(cx, |tree, _| tree.rename.is_some())
}

/// F2 on the reviewed task's row: the rename box has the title, all selected.
fn start(
    cx: &mut TestAppContext,
) -> (
    Entity<WorkspaceTree>,
    Entity<Dashboard>,
    &mut VisualTestContext,
) {
    let (root, dashboard, cx) = root(cx);
    let tree = parts(&root, cx).tree;
    tree.update_in(cx, |tree, window, cx| tree.focus_task(REVIEWED, window, cx));
    cx.run_until_parked();
    press(cx, "f2");
    assert!(renaming(&tree, cx));
    (tree, dashboard, cx)
}

fn renamed() -> Vec<(String, String)> {
    fake_daemon::shared().renamed()
}

#[gpui::test]
fn t27_enter_saves_and_the_focus_goes_back_to_the_row(cx: &mut TestAppContext) {
    let (tree, dashboard, cx) = start(cx);
    type_text(cx, "A new name");
    press(cx, "enter");
    assert!(!renaming(&tree, cx));
    assert_eq!(
        renamed(),
        [(REVIEWED.to_string(), "A new name".to_string())]
    );
    assert_eq!(title(&dashboard, cx), "A new name");
    assert!(is_focused(&tree, cx));
}

#[gpui::test]
fn t27_esc_drops_the_new_name(cx: &mut TestAppContext) {
    let (tree, dashboard, cx) = start(cx);
    let before = title(&dashboard, cx);
    type_text(cx, "Dropped");
    press(cx, "escape");
    assert!(!renaming(&tree, cx));
    assert!(renamed().is_empty());
    assert_eq!(title(&dashboard, cx), before);
    assert!(is_focused(&tree, cx));
}

#[gpui::test]
fn t27_a_blur_saves(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let parts = parts(&root, cx);
    let tree = parts.tree;
    tree.update_in(cx, |tree, window, cx| tree.focus_task(REVIEWED, window, cx));
    cx.run_until_parked();
    press(cx, "f2");
    type_text(cx, "Saved on blur");
    // The focus goes to the task list, outside the tree.
    let list = parts
        .tasks
        .read_with(cx, |tasks, cx| tasks.focus_handle(cx));
    cx.update(|window, cx| window.focus(&list, cx));
    cx.run_until_parked();
    assert!(!renaming(&tree, cx));
    assert_eq!(
        renamed(),
        [(REVIEWED.to_string(), "Saved on blur".to_string())]
    );
    assert_eq!(title(&dashboard, cx), "Saved on blur");
}

#[gpui::test]
fn t27_an_unchanged_title_sends_nothing(cx: &mut TestAppContext) {
    let (tree, dashboard, cx) = start(cx);
    let before = title(&dashboard, cx);
    press(cx, "enter");
    assert!(!renaming(&tree, cx));
    // The same title with spaces around it, typed again.
    press(cx, "f2");
    press(cx, "mod-a backspace");
    type_text(cx, &format!("  {before}  "));
    press(cx, "enter");
    let empty = format!("{:?}", renamed());
    // An empty title keeps the old one.
    press(cx, "f2");
    press(cx, "mod-a backspace");
    press(cx, "enter");
    assert_eq!(format!("{empty} then {:?}", renamed()), "[] then []");
    assert_eq!(title(&dashboard, cx), before);
}

/// Shift+F10 opens the row's menu (task row or project header); Esc closes it with the focus
/// back on the tree.
fn menu_on(key: String, cx: &mut TestAppContext) -> String {
    let (root, _, cx) = root(cx);
    let tree = parts(&root, cx).tree;
    tree.update_in(cx, |tree, window, cx| tree.focus_key(key, window, cx));
    cx.run_until_parked();
    press(cx, "shift-f10");
    let opened = cx.update(|_, cx| MenuLayer::open_menu(cx));
    let in_menu = opened.as_ref().is_some_and(|menu| is_focused(menu, cx));
    press(cx, "escape");
    let closed = cx.update(|_, cx| MenuLayer::open_menu(cx)).is_none();
    format!(
        "opened {}, focus in menu {in_menu}, esc closed {closed}, tree focused {}, sent {:?}",
        opened.is_some(),
        is_focused(&tree, cx),
        fake_daemon::shared().sent()
    )
}

#[gpui::test]
fn t23_shift_f10_opens_the_menu_of_a_task_row(cx: &mut TestAppContext) {
    assert_eq!(
        menu_on(format!("task:{REVIEWED}"), cx),
        "opened true, focus in menu true, esc closed true, tree focused true, sent []"
    );
}

#[gpui::test]
fn t23_shift_f10_opens_the_menu_of_a_project_header(cx: &mut TestAppContext) {
    assert_eq!(
        menu_on(format!("repo:{PROJECT}"), cx),
        "opened true, focus in menu true, esc closed true, tree focused true, sent []"
    );
}
