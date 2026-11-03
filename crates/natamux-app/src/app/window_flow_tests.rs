//! Workflow tests T3 and the focus part of T6 (plan 7.5), in the app's root view on the fake
//! daemon: the single keys type into the text boxes of the details panel (its own, and the commit
//! box and doc edit box of the docked Changes and Docs views) and the sidebar tree and call no
//! daemon; outside a box `a` approves the selected task once; a focus moved with
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
use crate::app::shots_files::doc_draft;
use crate::app::source::Source;
use crate::app::test_driver::{press, text_of, type_text};
use crate::app::test_views;
use crate::app::view_rules::DetailsView;
use crate::components::dialog::DialogLayer;
use crate::sidebar::workspace_tree::WorkspaceTree;
use crate::tasks::details_panel::DetailsPanel;
use crate::tasks::task_view::TaskView;

/// The single keys of the lists and the details panel (`a r e i c n j k / ?`).
const SINGLE_KEYS: &str = "areicnjk/?";
/// In review in the fake, so `a` approves it.
pub(crate) const REVIEWED: &str = "T-0143";

/// The root view as `window::open` makes it, with the reviewed task chosen and its details shown.
/// Other flow tests that need Root's key handlers start here too.
pub(crate) fn root(
    cx: &mut TestAppContext,
) -> (Entity<Root>, Entity<Dashboard>, &mut VisualTestContext) {
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

/// The views of the root that the flow tests drive.
pub(crate) struct Parts {
    pub details: Entity<DetailsPanel>,
    pub tasks: Entity<TaskView>,
    pub tree: Entity<WorkspaceTree>,
}

pub(crate) fn parts(root: &Entity<Root>, cx: &mut VisualTestContext) -> Parts {
    root.read_with(cx, |root, cx| Parts {
        details: root.details.clone(),
        tasks: root.tasks.clone(),
        tree: root.sidebar.read(cx).tree().clone(),
    })
}

fn handle_of(view: &Entity<impl Focusable>, cx: &mut VisualTestContext) -> FocusHandle {
    cx.update(|_, cx| view.focus_handle(cx))
}

fn focus(handle: &FocusHandle, cx: &mut VisualTestContext) {
    cx.update(|window, cx| window.focus(handle, cx));
    cx.run_until_parked();
}

/// Whether the details panel's key context has `not_editing`; None when the focus is not in it.
pub(crate) fn not_editing(cx: &mut VisualTestContext) -> Option<bool> {
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

/// Opens the reviewed task's workspace with `view` in the docked details panel.
fn dock(dashboard: &Entity<Dashboard>, view: DetailsView, cx: &mut VisualTestContext) {
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open(REVIEWED, cx);
        dashboard.set_details_view(view, cx);
        dashboard.set_details_open(true, cx);
    });
    cx.run_until_parked();
}

/// Today the single keys do not reach the docked commit box (see "Bugs found" of the MIT move):
/// the box sits under the panel's NatamuxDetails, whose context keeps not_editing because
/// `DetailsPanel::editing` knows only its own boxes. So `a` approves the task (`r` then has no
/// review to answer) and `e` opens the edit form, which takes the focus. The test records that;
/// S3 keeps it until the owner decides.
#[gpui::test]
fn t3_single_keys_are_taken_in_the_commit_box_of_the_docked_changes(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    dock(&dashboard, DetailsView::Changes, cx);
    let (details, changes) = root.read_with(cx, |root, cx| {
        let changes = root.workspace_view().read(cx).changes_view().clone();
        (root.details.clone(), changes)
    });
    let commit = changes.read_with(cx, |changes, _| changes.commit_box());
    let commit = commit.expect("the docked Changes view drew no commit box");
    let handle = handle_of(&commit, cx);
    focus(&handle, cx);
    press(cx, "mod-a backspace");
    let mut seen = vec![format!("not_editing {:?}", not_editing(cx))];
    let mut calls = 0;
    for keys in ["a", &SINGLE_KEYS[1..]] {
        type_text(cx, keys);
        let sent = fake_daemon::shared().sent();
        let new: Vec<_> = sent[calls..]
            .iter()
            .map(|(_, event)| event.clone())
            .collect();
        calls = sent.len();
        let form = details.read_with(cx, |details, cx| details.boxes(cx).1.len());
        let dialog = cx.update(|_, cx| DialogLayer::is_open(cx));
        let in_box = cx.update(|window, cx| handle.contains_focused(window, cx));
        seen.push(format!(
            "{keys}: box {:?}, sent {new:?}, form {form}, dialog {dialog}, in box {in_box}",
            text_of(&commit, cx)
        ));
    }
    assert_eq!(
        seen,
        [
            "not_editing Some(true)",
            "a: box \"\", sent [\"review.approved\"], form 0, dialog false, in box true",
            "reicnjk/?: box \"\", sent [], form 4, dialog false, in box false",
        ]
    );
}

#[gpui::test]
fn t3_single_keys_type_into_the_doc_edit_box_of_the_docked_docs(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let spec = "spec.md";
    cx.update(|_, cx| doc_draft(REVIEWED, spec, cx));
    dock(&dashboard, DetailsView::Progress, cx);
    dashboard.update(cx, |dashboard, cx| {
        dashboard.open_stage_file(REVIEWED, spec, cx)
    });
    cx.run_until_parked();
    let docs = root.read_with(cx, |root, cx| {
        root.workspace_view().read(cx).docs().cloned()
    });
    let edit = docs.and_then(|docs| docs.read_with(cx, |docs, _| docs.edit_box()));
    let edit = edit.expect("the Docs view drew no edit box");
    let mut wrong = Vec::new();
    if let Some(text) = typed_into(&edit, cx) {
        wrong.push(format!("doc edit box: has {text:?}"));
    }
    // The panel shows only the list; the picked file and its edit box are drawn in main, outside
    // the panel's NatamuxDetails context.
    let context = not_editing(cx);
    if context.is_some() {
        wrong.push(format!("not_editing of the panel: {context:?}"));
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
