//! Workflow tests T3 and the focus part of T6 (plan 7.5), in the app's root view on the fake
//! daemon: the single keys type into the text boxes of the details panel and the sidebar tree and
//! call no daemon; outside a box `a` approves the selected task once; a focus moved with
//! `window.focus` turns `not_editing` of the cached details panel off and on.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use editor::Editor;
use gpui::{Entity, FocusHandle, Focusable, TestAppContext, VisualTestContext};
use natamux_proto::tasks::task::TaskState;

use super::Root;
use crate::app::dashboard::{Dashboard, SavedView};
use crate::app::fake_daemon;
use crate::app::shortcuts;
use crate::app::source::Source;
use crate::app::test_driver::{press, text_of, type_text};
use crate::app::test_views;
use crate::app::view_rules::DetailsView;
use crate::components::dialog::DialogLayer;

/// The single keys of the lists and the details panel (`a r e i c n j k / ?`).
const SINGLE_KEYS: &str = "areicnjk/?";
/// In review in the fake, so `a` approves it.
const REVIEWED: &str = "T-0143";

/// The root view as `window::open` makes it, with the reviewed task chosen and its details shown.
fn root(cx: &mut TestAppContext) -> (Entity<Root>, Entity<Dashboard>, &mut VisualTestContext) {
    test_views::init(cx);
    cx.update(|cx| {
        shortcuts::bind(cx);
        crate::git::languages::init(cx);
    });
    let (root, cx) = cx.add_window_view(|window, cx| {
        let source = Ok(Arc::new(Source::FakeDaemon));
        Root::new(source, SavedView::default(), window, cx)
    });
    cx.update(|window, _| window.activate_window());
    cx.run_until_parked();
    let dashboard = root.read_with(cx, |root, _| root.dashboard.clone());
    dashboard.update(cx, |dashboard, cx| {
        dashboard.choose(REVIEWED, cx);
        dashboard.set_details_view(DetailsView::Details, cx);
    });
    cx.run_until_parked();
    (root, dashboard, cx)
}

fn handle_of(view: &Entity<impl Focusable>, cx: &mut VisualTestContext) -> FocusHandle {
    cx.update(|_, cx| view.focus_handle(cx))
}

fn focus(handle: &FocusHandle, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(handle, cx));
    cx.run_until_parked();
}

/// Whether the details panel's key context has `not_editing`; None when the focus is not in it.
fn not_editing(cx: &mut VisualTestContext) -> Option<bool> {
    cx.update(|window, _| {
        let stack = window.context_stack();
        let details = stack
            .iter()
            .find(|context| context.contains("NatamuxDetails"));
        details.map(|context| context.contains("not_editing"))
    })
}

/// What the single keys did besides typing: daemon calls, an open dialog, another task shown.
fn side_effects(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Vec<String> {
    let mut found = Vec::new();
    let sent = fake_daemon::shared().sent();
    if !sent.is_empty() {
        found.push(format!("daemon calls {sent:?}"));
    }
    if cx.update(|_, cx| DialogLayer::is_open(cx)) {
        found.push("a dialog opened".into());
    }
    let shown = dashboard.read_with(cx, |dashboard, _| {
        dashboard.details_task().map(|task| task.id.clone())
    });
    if shown.as_deref() != Some(REVIEWED) {
        found.push(format!("the details show {shown:?}"));
    }
    found
}

/// Types the single keys into the emptied box; the text it has when it is not just them.
fn typed_into(text_box: &Entity<Editor>, cx: &mut VisualTestContext) -> Option<String> {
    let handle = handle_of(text_box, cx);
    focus(&handle, cx);
    press(cx, "mod-a backspace");
    type_text(cx, SINGLE_KEYS);
    let text = text_of(text_box, cx);
    (text != SINGLE_KEYS).then_some(text)
}

#[gpui::test]
fn t3_single_keys_type_into_the_boxes_of_the_details_panel(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = root.read_with(cx, |root, _| root.details.clone());
    let mut wrong = Vec::new();
    let (subtask, _) = details.read_with(cx, |details, cx| details.boxes(cx));
    if let Some(text) = typed_into(&subtask, cx) {
        wrong.push(format!("subtask: has {text:?}"));
    }

    details.update(cx, |details, cx| details.start_edit(cx));
    cx.run_until_parked();
    let (_, form) = details.read_with(cx, |details, cx| details.boxes(cx));
    assert_eq!(form.len(), 4, "the edit form did not open");
    let names = ["title", "notes", "labels", "depends on"];
    for (name, text_box) in names.into_iter().zip(form) {
        if let Some(text) = typed_into(&text_box, cx) {
            wrong.push(format!("{name}: has {text:?}"));
        }
    }
    wrong.extend(side_effects(&dashboard, cx));
    assert!(wrong.is_empty(), "\n{}", wrong.join("\n"));
}

#[gpui::test]
fn t3_single_keys_type_into_the_rename_box_of_the_tree(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let tree = root.read_with(cx, |root, cx| root.sidebar.read(cx).tree().clone());
    tree.update_in(cx, |tree, window, cx| tree.focus_task(REVIEWED, window, cx));
    cx.run_until_parked();
    press(cx, "f2");
    type_text(cx, SINGLE_KEYS);
    press(cx, "enter");
    let title = dashboard.read_with(cx, |dashboard, _| {
        dashboard.data.task(REVIEWED).map(|task| task.title.clone())
    });
    assert_eq!(title.as_deref(), Some(SINGLE_KEYS));
    let found = side_effects(&dashboard, cx);
    assert!(found.is_empty(), "{found:?}");
}

#[gpui::test]
fn t3_a_outside_a_box_approves_the_selected_task_once(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let tasks = root.read_with(cx, |root, _| root.tasks.clone());
    let table = handle_of(&tasks, cx);
    focus(&table, cx);
    press(cx, "a");
    let sent = fake_daemon::shared().sent();
    assert_eq!(
        sent,
        [(REVIEWED.to_string(), "review.approved".to_string())]
    );
    let state = dashboard.read_with(cx, |dashboard, _| {
        dashboard.data.task(REVIEWED).map(|task| task.state)
    });
    assert_eq!(state, Some(TaskState::Done));
}

#[gpui::test]
fn t6_window_focus_turns_not_editing_of_the_cached_details_panel_off_and_on(
    cx: &mut TestAppContext,
) {
    let (root, _, cx) = root(cx);
    let details = root.read_with(cx, |root, _| root.details.clone());
    let (subtask, _) = details.read_with(cx, |details, cx| details.boxes(cx));
    let in_box = handle_of(&subtask, cx);
    let panel = handle_of(&details, cx);
    let mut seen = Vec::new();
    for handle in [&in_box, &panel, &in_box] {
        focus(handle, cx);
        seen.push(not_editing(cx));
    }
    assert_eq!(seen, [Some(false), Some(true), Some(false)]);
}
