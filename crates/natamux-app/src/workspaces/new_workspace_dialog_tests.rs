//! Workflow test T20 (plan 7.5): Start a task in a project whose commands wait for approval goes
//! on after Not now and after Approve; a 409 on Approve shows the changed commands; a dialog
//! closed while its upload is read still opens the task; the Tab ring follows what the dialog
//! shows; and its selects open their menus over it.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::PathBuf;

use gpui::{DismissEvent, Entity, Focusable, TestAppContext, VisualTestContext, point, px};
use natamux_proto::tasks::task::Task;

use super::{NewWorkspaceDialog, Pick, open_with};
use crate::app::dashboard::Dashboard;
use crate::app::fake_daemon;
use crate::app::test_driver::{activate, press, type_text};
use crate::app::test_views::{self, PROJECT};
use crate::components::dialog::{DialogLayer, DialogView};
use crate::components::menu::MenuLayer;
use crate::projects::repo_trust_dialog::RepoTrustDialog;
use crate::projects::repo_trust_dialog::rules::CHANGED;

const TITLE: &str = "Fix the login redirect";

/// The dialog in the fake's project, once it read the project's branches, settings and skills.
fn open_dialog(
    dashboard: &Entity<Dashboard>,
    cx: &mut VisualTestContext,
) -> Entity<NewWorkspaceDialog> {
    let dialog = cx.update(|window, cx| {
        open_with(dashboard, Some(PROJECT.into()), None, window, cx).expect("the dialog opened")
    });
    cx.run_until_parked();
    dialog
}

fn started(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) -> Vec<Task> {
    dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        tasks.filter(|task| task.title == TITLE).cloned().collect()
    })
}

fn trust_dialog(cx: &mut VisualTestContext) -> Option<Entity<RepoTrustDialog>> {
    let top = cx.update(|_, cx| DialogLayer::top_view(cx))?;
    top.downcast::<RepoTrustDialog>().ok()
}

fn is_open(cx: &mut VisualTestContext) -> bool {
    cx.update(|_, cx| DialogLayer::is_open(cx))
}

fn trusted() -> bool {
    let view = fake_daemon::shared().repo_trust(PROJECT);
    view.expect("the project").trusted
}

/// Opens the dialog in the untrusted project, types the title and presses Mod+Enter: the trust
/// dialog opens over it.
fn start_untrusted(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    fake_daemon::shared().distrust_repo(PROJECT);
    open_dialog(dashboard, cx);
    type_text(cx, TITLE);
    press(cx, "mod-enter");
    assert!(trust_dialog(cx).is_some(), "the trust dialog did not open");
    assert!(started(dashboard, cx).is_empty());
}

/// The task started once, it is open in main, and every dialog closed.
fn started_once(dashboard: &Entity<Dashboard>, cx: &mut VisualTestContext) {
    let tasks = started(dashboard, cx);
    assert_eq!(tasks.len(), 1, "{tasks:?}");
    let open = dashboard.read_with(cx, |dashboard, _| dashboard.open_task.clone());
    assert_eq!(open.as_deref(), Some(tasks[0].id.as_str()));
    assert!(!is_open(cx));
}

#[gpui::test]
fn t20_not_now_in_the_trust_dialog_still_starts_the_task(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    start_untrusted(&dashboard, cx);
    // Not now has the first focus.
    activate(cx);
    cx.run_until_parked();
    started_once(&dashboard, cx);
    assert!(!trusted());
}

#[gpui::test]
fn t20_approve_in_the_trust_dialog_starts_the_task(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    start_untrusted(&dashboard, cx);
    press(cx, "tab");
    activate(cx);
    cx.run_until_parked();
    started_once(&dashboard, cx);
    assert!(trusted());
}

#[gpui::test]
fn t20_a_409_on_approve_shows_the_changed_commands(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    start_untrusted(&dashboard, cx);
    let trust = trust_dialog(cx).expect("open");
    let before = trust.read_with(cx, |trust, _| trust.shown().0.clone());
    fake_daemon::shared().change_repo_commands(PROJECT);
    press(cx, "mod-enter");
    let (view, error) = trust.read_with(cx, |trust, _| {
        let (view, error) = trust.shown();
        (view.clone(), error.map(str::to_owned))
    });
    assert_eq!(error.as_deref(), Some(CHANGED));
    assert_ne!(view.hash, before.hash);
    assert_eq!(view.commands.len(), before.commands.len() + 1);
    assert!(trust_dialog(cx).is_some());
    assert!(started(&dashboard, cx).is_empty());
    assert!(!trusted());
    // The new commands, approved, let the start go on.
    press(cx, "mod-enter");
    started_once(&dashboard, cx);
    assert!(trusted());
}

#[gpui::test]
fn t20_a_dialog_closed_while_the_upload_is_read_still_opens_the_task(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open_dialog(&dashboard, cx);
    type_text(cx, TITLE);
    let file = PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml"));
    dialog.update(cx, |dialog, cx| dialog.add_paths(vec![file], cx));
    cx.run_until_parked();
    assert_eq!(dialog.read_with(cx, |dialog, _| dialog.files.len()), 1);
    // What Start does once the trust check is done; the close button emits DismissEvent before
    // the upload task ran.
    dialog.update(cx, |dialog, cx| {
        dialog.start(cx);
        cx.emit(DismissEvent);
    });
    assert!(!is_open(cx));
    assert!(started(&dashboard, cx).is_empty());
    cx.run_until_parked();
    started_once(&dashboard, cx);
}

/// The ring, each stop named by its part.
fn ring_names(dialog: &Entity<NewWorkspaceDialog>, cx: &mut VisualTestContext) -> Vec<String> {
    dialog.read_with(cx, |dialog, cx| {
        let stops = &dialog.stops;
        let parts = [
            ("title", dialog.title.focus_handle(cx)),
            ("source", stops.source.clone()),
            ("skill", stops.skill.clone()),
            ("notes", dialog.notes.focus_handle(cx)),
            ("reference", dialog.reference.focus_handle(cx)),
            ("files", stops.files.clone()),
            ("project", stops.project.clone()),
            ("branch", stops.branch.clone()),
            ("branch name", dialog.branch_name.focus_handle(cx)),
            ("agent", stops.agent.clone()),
            ("cancel", stops.cancel.clone()),
            ("submit", stops.submit.clone()),
        ];
        dialog
            .tab_ring(cx)
            .iter()
            .map(|stop| {
                let part = parts.iter().find(|(_, handle)| handle == stop);
                part.map_or("unnamed", |(name, _)| *name).to_string()
            })
            .collect()
    })
}

#[gpui::test]
fn t20_the_tab_ring_follows_what_the_dialog_shows(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    // Before the project's reads come back.
    let dialog = cx.update(|window, cx| {
        open_with(&dashboard, Some(PROJECT.into()), None, window, cx).expect("the dialog opened")
    });
    let mut seen = vec![format!("opened: {:?}", ring_names(&dialog, cx))];
    cx.run_until_parked();
    seen.push(format!("read: {:?}", ring_names(&dialog, cx)));
    dialog.update(cx, |dialog, cx| {
        dialog.picked_source = "skill:".into();
        cx.notify();
    });
    cx.run_until_parked();
    seen.push(format!("agent skill: {:?}", ring_names(&dialog, cx)));
    assert_eq!(
        seen,
        [
            "opened: [\"title\", \"notes\", \"files\", \"project\", \"branch name\", \"agent\", \
             \"cancel\", \"submit\"]",
            "read: [\"title\", \"source\", \"notes\", \"files\", \"project\", \"branch\", \
             \"branch name\", \"agent\", \"cancel\", \"submit\"]",
            "agent skill: [\"title\", \"source\", \"skill\", \"reference\", \"files\", \
             \"project\", \"branch\", \"branch name\", \"agent\", \"cancel\", \"submit\"]",
        ]
    );
    // Tab follows the ring as it is now.
    let ring = dialog.read_with(cx, |dialog, cx| dialog.tab_ring(cx));
    cx.update(|window, cx| window.focus(&ring[0], cx));
    cx.run_until_parked();
    let mut wrong = Vec::new();
    for (step, want) in ring.iter().enumerate().skip(1) {
        press(cx, "tab");
        if !cx.update(|window, _| want.is_focused(window)) {
            wrong.push(format!("Tab {step} missed its stop"));
        }
    }
    assert!(wrong.is_empty(), "{wrong:?}");
}

/// What a pick changed in the dialog, as a text.
fn picked(dialog: &Entity<NewWorkspaceDialog>, pick: Pick, cx: &mut VisualTestContext) -> String {
    dialog.read_with(cx, |dialog, _| match pick {
        Pick::Source => dialog.picked_source.clone(),
        Pick::Skill => dialog.skill.clone(),
        Pick::Project => dialog.picked_repo.clone(),
        Pick::Branch => dialog.base.clone(),
        Pick::Agent => dialog.agent.clone(),
    })
}

/// The stop of each select, where the menu gives the focus back.
fn stop_of(
    dialog: &Entity<NewWorkspaceDialog>,
    pick: Pick,
    cx: &mut VisualTestContext,
) -> gpui::FocusHandle {
    dialog.read_with(cx, |dialog, _| {
        let stops = &dialog.stops;
        match pick {
            Pick::Source => stops.source.clone(),
            Pick::Skill => stops.skill.clone(),
            Pick::Project => stops.project.clone(),
            Pick::Branch => stops.branch.clone(),
            Pick::Agent => stops.agent.clone(),
        }
    })
}

fn open_menu(dialog: &Entity<NewWorkspaceDialog>, pick: Pick, cx: &mut VisualTestContext) {
    let stop = stop_of(dialog, pick, cx);
    cx.update(|window, cx| window.focus(&stop, cx));
    dialog.update_in(cx, |dialog, window, cx| {
        dialog.open_menu(pick, point(px(200.), px(200.)), window, cx)
    });
    cx.run_until_parked();
}

/// Menus over the dialog (T23): Esc closes the menu and leaves the dialog, with the focus back
/// on the select; a menu opens with the checked option picked (else the first), so Down then
/// Enter picks the one after it.
#[gpui::test]
fn t20_the_selects_open_their_menus_over_the_dialog(cx: &mut TestAppContext) {
    let (_, dashboard, cx) = test_views::init_opener(cx);
    let dialog = open_dialog(&dashboard, cx);
    dialog.update(cx, |dialog, cx| {
        dialog.picked_source = "skill:".into();
        cx.notify();
    });
    cx.run_until_parked();
    let mut seen = Vec::new();
    for pick in [
        Pick::Source,
        Pick::Skill,
        Pick::Project,
        Pick::Branch,
        Pick::Agent,
    ] {
        open_menu(&dialog, pick, cx);
        let opened = cx.update(|_, cx| MenuLayer::open_menu(cx)).is_some();
        press(cx, "escape");
        let stop = stop_of(&dialog, pick, cx);
        let back = cx.update(|window, _| stop.is_focused(window));
        let closed = cx.update(|_, cx| MenuLayer::open_menu(cx)).is_none();
        let dialog_open = is_open(cx);
        open_menu(&dialog, pick, cx);
        press(cx, "down enter");
        seen.push(format!(
            "{pick:?}: opened {opened}, esc closed {closed}, dialog {dialog_open}, focus back \
             {back}; down enter {:?}",
            picked(&dialog, pick, cx)
        ));
        // A pick of Source may change the ring; keep the agent skill shown.
        dialog.update(cx, |dialog, cx| {
            dialog.picked_source = "skill:".into();
            cx.notify();
        });
        cx.run_until_parked();
    }
    assert!(is_open(cx));
    assert_eq!(
        seen,
        [
            "Source: opened true, esc closed true, dialog true, focus back true; down enter \
             \"manual\"",
            "Skill: opened true, esc closed true, dialog true, focus back true; down enter \
             \"natamux-stage-check\"",
            "Project: opened true, esc closed true, dialog true, focus back true; down enter \
             \"natamux-site\"",
            "Branch: opened true, esc closed true, dialog true, focus back true; down enter \
             \"dev\"",
            "Agent: opened true, esc closed true, dialog true, focus back true; down enter \
             \"codex\"",
        ]
    );
}
