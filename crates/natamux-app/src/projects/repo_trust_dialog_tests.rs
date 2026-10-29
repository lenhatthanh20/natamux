//! The dialog draws every command and script, Not now runs what waits once, and a refused
//! approval keeps the dialog open with the daemon's reason.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;

use gpui::{Entity, TestAppContext, VisualTestContext};

use super::*;
use crate::app::test_views;

fn view() -> RepoTrustView {
    RepoTrustView {
        trusted: false,
        hash: "ab12".into(),
        commands: vec![RepoCommand {
            place: "Stage test: check".into(),
            text: "cargo test --locked".into(),
        }],
        setup: vec![SetupScript {
            file: ".natamux-setup".into(),
            text: "pnpm install\ncp .env.example .env\n".into(),
        }],
    }
}

/// The dialog in a window of its own; `repo_id` is not a project of the fake, so an approval
/// changes nothing shared.
fn shown<'a>(
    repo_id: &str,
    ran: &Rc<Cell<usize>>,
    cx: &'a mut TestAppContext,
) -> (Entity<RepoTrustDialog>, &'a mut VisualTestContext) {
    test_views::init(cx);
    let count = ran.clone();
    let then: Then = Rc::new(move |_: &mut Window, _: &mut App| {
        count.set(count.get() + 1);
    });
    let repo_id = repo_id.to_owned();
    let (dialog, cx) = cx.add_window_view(move |_, cx| RepoTrustDialog {
        source: Arc::new(Source::FakeDaemon),
        repo_id,
        name: "Shop".into(),
        view: view(),
        then: Some(then),
        busy: false,
        error: None,
        focus: cx.focus_handle(),
        not_now: cx.focus_handle(),
        approve: cx.focus_handle(),
    });
    cx.run_until_parked();
    (dialog, cx)
}

#[gpui::test]
fn not_now_runs_what_waits_once(cx: &mut TestAppContext) {
    let ran = Rc::new(Cell::new(0));
    let (dialog, cx) = shown("no-such-project", &ran, cx);

    dialog.update_in(cx, |dialog, window, cx| {
        assert!(dialog.cancel(window, cx), "Esc must be Not now");
        dialog.close(window, cx);
    });

    assert_eq!(ran.get(), 1);
}

// Start a task: with nothing to approve, `then` updates the view that called `review` (v1.0.7
// panicked there with a double lease).
#[gpui::test]
fn what_waits_may_update_the_view_that_asked(cx: &mut TestAppContext) {
    test_views::init(cx);
    let ran = Rc::new(Cell::new(0));
    let (host, cx) = cx.add_window_view(|_, _| test_views::Counted { draws: 0 });

    let count = ran.clone();
    host.update_in(cx, |_, window, cx| {
        let this = cx.weak_entity();
        let then: Then = Rc::new(move |_: &mut Window, cx: &mut App| {
            this.update(cx, |_, _| count.set(count.get() + 1)).ok();
        });
        let source = Arc::new(Source::FakeDaemon);
        review(&source, "no-such-project", "Shop", Some(then), window, cx);
    });
    cx.run_until_parked();

    assert_eq!(ran.get(), 1);
}

#[gpui::test]
fn a_refused_approval_shows_the_reason_and_stays(cx: &mut TestAppContext) {
    let ran = Rc::new(Cell::new(0));
    let (dialog, cx) = shown("no-such-project", &ran, cx);

    dialog.update_in(cx, |dialog, window, cx| dialog.approve(window, cx));
    cx.run_until_parked();

    dialog.read_with(cx, |dialog, _| {
        assert!(!dialog.busy);
        assert_eq!(
            dialog.error.as_deref(),
            Some("Project no-such-project not found")
        );
        assert_eq!(dialog.view, view());
    });
    assert_eq!(ran.get(), 0);
}
