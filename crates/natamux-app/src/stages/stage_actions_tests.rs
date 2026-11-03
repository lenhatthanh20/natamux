//! Workflow test T24 (plan 7.5), in the app's root view on the fake daemon: what each button of
//! the current stage card sends, that every button is off while a request is on its way (a
//! double click sends one), the agent picker, and what `a` sends next to the Approve button.
//! The tests call the panel's `on_command` with the command of each button;
//! `stage_actions_row_tests.rs` clicks the drawn buttons and checks the command each one sends.
//! (A child of the details panel, so it reaches the panel's state.)

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use gpui::{Entity, Focusable, TestAppContext, VisualTestContext, point, px};

use super::DetailsPanel;
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::press;
use crate::app::window::flow_tests::{REVIEWED, parts, root};
use crate::components::dialog::DialogLayer;
use crate::stages::send_back_dialog::SendBackDialog;
use crate::stages::stage_choices::open_agent_menu;
use crate::tasks::details_commands::PanelCommand;

/// The current stage of the reviewed task, a manual review.
const STAGE: &str = "review";

fn run(details: &Entity<DetailsPanel>, commands: Vec<PanelCommand>, cx: &mut VisualTestContext) {
    let on = details.update(cx, |panel, cx| panel.on_command(cx));
    cx.update(|window, cx| {
        for command in commands {
            on(command, window, cx);
        }
    });
}

/// The events the fake logged for the task since `from`, as "kind stage".
fn events_since(from: usize) -> Vec<String> {
    let events = fake_daemon::shared().list_events(REVIEWED).unwrap();
    events
        .iter()
        .skip(from)
        .map(|stored| {
            let stage = stored.event.payload.get("stage");
            let stage = stage.and_then(|value| value.as_str()).unwrap_or("-");
            format!("{} {stage}", stored.event.kind)
        })
        .collect()
}

fn event_count() -> usize {
    fake_daemon::shared().list_events(REVIEWED).unwrap().len()
}

/// The task's state, stage and agent, and the task open in main.
fn task_line(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> String {
    dashboard.read_with(cx, |dashboard, _| {
        let task = dashboard.data.task(REVIEWED).expect("a task");
        format!(
            "{:?} at {}, agent {:?}, open {:?}",
            task.state, task.stage, task.agent, dashboard.open_task
        )
    })
}

/// What one button does: the stage events, the other calls, the dialog it opens, the task after.
fn after_click(command: PanelCommand, cx: &mut TestAppContext) -> Vec<String> {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let from = event_count();
    run(&details, vec![command], cx);
    cx.run_until_parked();
    let dialog = cx.update(|_, cx| DialogLayer::top_view(cx));
    let send_back = dialog.is_some_and(|view| view.downcast::<SendBackDialog>().is_ok());
    vec![
        format!("events {:?}", events_since(from)),
        format!("sent {:?}", fake_daemon::shared().sent()),
        format!("send back dialog {send_back}"),
        task_line(&dashboard, cx),
    ]
}

#[gpui::test]
fn t24_approve_sends_approve_of_the_stage(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::Approve(STAGE.into()), cx);
    assert_eq!(
        seen,
        [
            "events [\"review.approved review\", \"stage.started release\"]",
            "sent []",
            "send back dialog false",
            "Running at release, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_skip_sends_skip_of_the_stage(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::Skip(STAGE.into()), cx);
    assert_eq!(
        seen,
        [
            "events [\"stage.skipped review\", \"stage.started release\"]",
            "sent []",
            "send back dialog false",
            "Running at release, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_mark_as_done_sends_report_of_the_stage(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::Report(STAGE.into()), cx);
    assert_eq!(
        seen,
        [
            "events [\"stage.reported review\"]",
            "sent []",
            "send back dialog false",
            "Review at review, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_start_sends_start_of_the_stage(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::Start(STAGE.into()), cx);
    assert_eq!(
        seen,
        [
            "events [\"stage.started review\"]",
            "sent []",
            "send back dialog false",
            "Running at review, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_send_back_opens_its_dialog_and_sends_nothing(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::SendBack(STAGE.into()), cx);
    assert_eq!(
        seen,
        [
            "events []",
            "sent []",
            "send back dialog true",
            "Review at review, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_resume_sends_agent_resumed(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::Resume, cx);
    assert_eq!(
        seen,
        [
            "events []",
            "sent [(\"T-0143\", \"agent.resumed\")]",
            "send back dialog false",
            "Running at review, agent Some(\"claude-code\"), open None",
        ]
    );
}

#[gpui::test]
fn t24_start_agent_starts_the_task_agent_and_opens_the_task(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::StartStageAgent, cx);
    assert_eq!(
        seen,
        [
            "events []",
            "sent []",
            "send back dialog false",
            "Running at review, agent Some(\"claude-code\"), open Some(\"T-0143\")",
        ]
    );
}

#[gpui::test]
fn t24_answer_in_terminal_opens_the_task(cx: &mut TestAppContext) {
    let seen = after_click(PanelCommand::AnswerInTerminal, cx);
    assert_eq!(
        seen,
        [
            "events []",
            "sent []",
            "send back dialog false",
            "Review at review, agent Some(\"claude-code\"), open Some(\"T-0143\")",
        ]
    );
}

#[gpui::test]
fn t24_every_button_is_off_while_a_request_is_on_its_way(cx: &mut TestAppContext) {
    let (root, _, cx) = root(cx);
    let details = parts(&root, cx).details;
    let busy = |details: &Entity<DetailsPanel>, cx: &mut VisualTestContext| {
        details.read_with(cx, |panel, _| panel.shown.as_ref().is_some_and(|s| s.busy))
    };
    let from = event_count();
    // A double click, then the other buttons, all before the daemon answers.
    run(
        &details,
        vec![
            PanelCommand::Approve(STAGE.into()),
            PanelCommand::Approve(STAGE.into()),
        ],
        cx,
    );
    assert!(busy(&details, cx));
    run(
        &details,
        vec![
            PanelCommand::Skip(STAGE.into()),
            PanelCommand::Report(STAGE.into()),
            PanelCommand::Start(STAGE.into()),
            PanelCommand::StartStageAgent,
        ],
        cx,
    );
    cx.run_until_parked();
    assert!(!busy(&details, cx));
    let events = events_since(from);
    let approved = events.iter().filter(|e| e.starts_with("review.approved"));
    assert_eq!(approved.count(), 1, "{events:?}");
    let others = ["stage.skipped", "stage.reported", "stage.started review"];
    let wrong: Vec<_> = events
        .iter()
        .filter(|e| others.iter().any(|other| e.starts_with(other)))
        .collect();
    assert!(wrong.is_empty(), "{wrong:?}");
}

#[gpui::test]
fn t24_the_agent_picker_picks_the_agent_that_starts(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let on = details.update(cx, |panel, cx| panel.on_command(cx));
    cx.update(|window, cx| open_agent_menu(point(px(200.), px(200.)), on, window, cx));
    cx.run_until_parked();
    // The first agent is picked on open; two Downs pick the third.
    press(cx, "down down enter");
    let agent = details.read_with(cx, |panel, _| {
        panel.shown.as_ref().and_then(|shown| shown.agent.clone())
    });
    run(&details, vec![PanelCommand::StartStageAgent], cx);
    cx.run_until_parked();
    let seen = format!("picked {agent:?}; {}", task_line(&dashboard, cx));
    assert_eq!(
        seen,
        "picked Some(\"gemini\"); Running at review, agent Some(\"gemini\"), open Some(\"T-0143\")"
    );
}

/// `a` with the focus on the panel, next to a click on Approve (`t24_approve_...`): the key sends
/// the task's `review.approved` event, the button the stage's approve. "Done" comes from the
/// fake (`state_after`); the daemon gives an event with no stage to the one stage it fits, which
/// then moves on as after the button (natamux-daemon `apply_group.rs`).
#[gpui::test]
fn t24_a_on_the_panel_sends_the_review_approved_event(cx: &mut TestAppContext) {
    let (root, dashboard, cx) = root(cx);
    let details = parts(&root, cx).details;
    let handle = details.read_with(cx, |panel, cx| panel.focus_handle(cx));
    cx.update(|window, cx| window.focus(&handle, cx));
    cx.run_until_parked();
    let from = event_count();
    press(cx, "a");
    let seen = vec![
        format!("events {:?}", events_since(from)),
        format!("sent {:?}", fake_daemon::shared().sent()),
        task_line(&dashboard, cx),
    ];
    assert_eq!(
        seen,
        [
            "events []",
            "sent [(\"T-0143\", \"review.approved\")]",
            "Done at review, agent Some(\"claude-code\"), open None",
        ]
    );
}
