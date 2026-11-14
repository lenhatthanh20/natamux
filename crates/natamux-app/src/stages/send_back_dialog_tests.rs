//! Workflow test T10 (plan 7.5): the Send back dialog picks the last earlier stage, sends
//! `Reopen` with the trimmed reason (none when empty) on Mod+Enter, ignores a second send and Esc
//! while it waits, stays open on an error, closes on Esc with the focus back at the opener, and
//! its picker changes the target.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, TestAppContext, VisualTestContext};
use natamux_proto::stages::progress::{StageProgress, StageStatus};
use natamux_proto::stages::schema::{Gate, StageKind};
use natamux_proto::tasks::task::Task;
use serde_json::{Map, Value};

use super::{SendBackDialog, open};
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::{activate, click_on, is_focused, press, press_now, type_text};
use crate::app::test_views;
use crate::components::dialog::DialogLayer;

/// In review in the fake.
const TASK: &str = "T-0143";

fn stage(id: &str, kind: StageKind, status: StageStatus) -> StageProgress {
    StageProgress {
        stage: id.into(),
        name: id.into(),
        kind,
        gate: Gate::Auto,
        output: None,
        group: None,
        path: None,
        because: None,
        status,
        state: None,
        rounds: 0,
        started_at: None,
        ended_at: None,
        approved_by: None,
        passes: vec![],
        reasons: vec![],
        sent_back_for: None,
        sent_back_by_ci: false,
    }
}

/// Plan and code are done; review is the current stage, so both can be picked.
fn progress() -> Vec<StageProgress> {
    vec![
        stage("plan", StageKind::Agent, StageStatus::Done),
        stage("code", StageKind::Agent, StageStatus::Done),
        stage("review", StageKind::Review, StageStatus::Current),
    ]
}

fn task_of(dashboard: &Entity<Dashboard>, id: &str, cx: &mut VisualTestContext) -> Task {
    dashboard.read_with(cx, |dashboard, _| {
        dashboard
            .data
            .task(id)
            .cloned()
            .expect("a task of the fake")
    })
}

fn open_for(
    dashboard: &Entity<Dashboard>,
    task: &Task,
    cx: &mut VisualTestContext,
) -> Entity<SendBackDialog> {
    let dialog = cx.update(|window, cx| {
        open(dashboard, task, "review", &progress(), window, cx).expect("the stage is there")
    });
    cx.run_until_parked();
    dialog
}

/// The payloads of the `stage.reopened` events a person sent. The fake's own send back of the
/// task, by an agent, has no `from`.
fn reopened(task: &str) -> Vec<Map<String, Value>> {
    let events = fake_daemon::shared().list_events(task).unwrap_or_default();
    events
        .into_iter()
        .filter(|stored| stored.event.kind == "stage.reopened")
        .filter(|stored| stored.event.payload.contains_key("from"))
        .map(|stored| stored.event.payload)
        .collect()
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| DialogLayer::is_open(cx))
}

#[gpui::test]
fn t10_mod_enter_sends_the_default_target_with_the_trimmed_reason(cx: &mut TestAppContext) {
    let (opener, dashboard, cx) = test_views::init_opener(cx);
    let task = task_of(&dashboard, TASK, cx);
    let dialog = open_for(&dashboard, &task, cx);
    assert_eq!(dialog.read_with(cx, |dialog, _| dialog.to.clone()), "code");

    type_text(cx, "  Add the cases.  ");
    press(cx, "mod-enter");
    let sent = reopened(TASK);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].get("to"), Some(&Value::from("code")));
    assert_eq!(sent[0].get("reason"), Some(&Value::from("Add the cases.")));
    assert!(!is_open(cx));
    assert!(is_focused(&opener, cx));
}

#[gpui::test]
fn t10_an_empty_reason_sends_none(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let task = task_of(&dashboard, TASK, cx);
    open_for(&dashboard, &task, cx);
    type_text(cx, "   ");
    press(cx, "mod-enter");
    let sent = reopened(TASK);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].get("to"), Some(&Value::from("code")));
    assert_eq!(sent[0].get("reason"), None, "{sent:?}");
    assert!(!is_open(cx));
}

#[gpui::test]
fn t10_busy_ignores_a_second_send_and_esc(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let task = task_of(&dashboard, TASK, cx);
    let dialog = open_for(&dashboard, &task, cx);
    type_text(cx, "Again");
    // The answer waits until the test lets the tasks run.
    press_now(cx, "mod-enter");
    assert!(dialog.read_with(cx, |dialog, _| dialog.busy));
    press_now(cx, "escape");
    assert!(is_open(cx), "Esc closed a busy dialog");
    press_now(cx, "mod-enter");
    // The Send back button is off while busy; its click handler checks again.
    dialog.update_in(cx, |dialog, window, cx| {
        crate::components::dialog::DialogView::submit(dialog, window, cx)
    });
    cx.run_until_parked();
    assert_eq!(reopened(TASK).len(), 1);
    assert!(!is_open(cx));
}

#[gpui::test]
fn t10_an_error_keeps_the_dialog_and_the_reason(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    // The fake has no such task, so the daemon answers 404.
    let missing = Task {
        id: "T-9999".into(),
        ..task_of(&dashboard, TASK, cx)
    };
    let dialog = open_for(&dashboard, &missing, cx);
    type_text(cx, "Fix it");
    press(cx, "mod-enter");
    assert!(is_open(cx));
    let (busy, reason) = dialog.read_with(cx, |dialog, cx| {
        (dialog.busy, dialog.reason.read(cx).text(cx))
    });
    assert!(!busy);
    assert_eq!(reason, "Fix it");
    assert!(reopened("T-9999").is_empty());
    press(cx, "escape");
    assert!(!is_open(cx));
}

#[gpui::test]
fn t10_esc_closes_and_the_focus_goes_back(cx: &mut TestAppContext) {
    let (opener, dashboard, cx) = test_views::init_opener(cx);
    let task = task_of(&dashboard, TASK, cx);
    let dialog = open_for(&dashboard, &task, cx);
    let reason = dialog.read_with(cx, |dialog, _| dialog.reason.clone());
    assert!(is_focused(&reason, cx));
    press(cx, "escape");
    assert!(!is_open(cx));
    assert!(is_focused(&opener, cx));
    assert!(reopened(TASK).is_empty());
}

#[gpui::test]
fn t10_the_picker_changes_the_target(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let task = task_of(&dashboard, TASK, cx);
    let dialog = open_for(&dashboard, &task, cx);
    // The picker is the stop before the reason.
    press(cx, "shift-tab");
    let pick = dialog.read_with(cx, |dialog, _| dialog.pick_focus.clone());
    assert!(cx.update(|window, _| pick.is_focused(window)));
    activate(cx);
    click_on(cx, "MENU_ITEM-plan");
    assert_eq!(dialog.read_with(cx, |dialog, _| dialog.to.clone()), "plan");
    // The menu gives the focus back to the picker, inside the dialog.
    assert!(cx.update(|window, _| pick.is_focused(window)));
    press(cx, "mod-enter");
    let sent = reopened(TASK);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].get("to"), Some(&Value::from("plan")));
    assert!(!is_open(cx));
}
