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

/// Workflow test T21 (plan 7.5): the dialog opened by `review` over an open dialog, in an app
/// window with the dialog layer. Not now, Approve and Esc each run `then` once, and `then` may
/// update the view that asked; the dialog under it stays open.
mod flow {
    use std::cell::Cell;
    use std::rc::Rc;
    use std::sync::Arc;

    use gpui::{AppContext as _, Entity, EntityId, TestAppContext, VisualTestContext};
    use ui::prelude::*;

    use super::super::{RepoTrustDialog, Then, review};
    use crate::app::fake_daemon;
    use crate::app::source::Source;
    use crate::app::test_driver::{activate, press, press_now};
    use crate::app::test_views::{self, Counted, PROJECT};
    use crate::components::confirm::Confirm;
    use crate::components::dialog::DialogLayer;

    /// What a test checks after the answer.
    struct Asked {
        host: Entity<Counted>,
        ran: Rc<Cell<usize>>,
        under: EntityId,
    }

    /// A confirm is open; the host view asks for the fake project's commands, which wait for
    /// approval, with a `then` that updates the host.
    fn asked(cx: &mut TestAppContext) -> (Asked, &mut VisualTestContext) {
        let (host, _, cx) = test_views::init_app(cx, |_, _, cx| cx.new(|_| Counted { draws: 0 }));
        cx.update(|window, cx| {
            Confirm::new("Under", "It stays.", "Go", |_, _| {}).open(window, cx)
        });
        cx.run_until_parked();
        let under = cx
            .update(|_, cx| DialogLayer::top_id(cx))
            .expect("the confirm");
        fake_daemon::shared().distrust_repo(PROJECT);
        let ran = Rc::new(Cell::new(0));
        let count = ran.clone();
        host.update_in(cx, |_, window, cx| {
            let this = cx.weak_entity();
            let then: Then = Rc::new(move |_: &mut Window, cx: &mut App| {
                this.update(cx, |host, _| {
                    host.draws += 100;
                    count.set(count.get() + 1);
                })
                .ok();
            });
            let source = Arc::new(Source::FakeDaemon);
            review(&source, PROJECT, "Natamux", Some(then), window, cx);
        });
        cx.run_until_parked();
        let top = cx.update(|_, cx| DialogLayer::top_view(cx));
        let top = top.and_then(|view| view.downcast::<RepoTrustDialog>().ok());
        assert!(
            top.is_some(),
            "the trust dialog did not open over the confirm"
        );
        (Asked { host, ran, under }, cx)
    }

    fn trusted() -> bool {
        let view = fake_daemon::shared().repo_trust(PROJECT);
        view.expect("the project").trusted
    }

    /// `then` ran once, it updated the host, and the confirm under is the top dialog again.
    fn ran_once_over(asked: &Asked, cx: &mut VisualTestContext) {
        assert_eq!(asked.ran.get(), 1);
        assert!(asked.host.read_with(cx, |host, _| host.draws >= 100));
        assert_eq!(
            cx.update(|_, cx| DialogLayer::top_id(cx)),
            Some(asked.under)
        );
    }

    #[gpui::test]
    fn t21_not_now_runs_then_once_and_the_dialog_under_stays(cx: &mut TestAppContext) {
        let (asked, cx) = asked(cx);
        // Not now has the first focus: Space clicks it.
        activate(cx);
        cx.run_until_parked();
        ran_once_over(&asked, cx);
        assert!(!trusted());
    }

    #[gpui::test]
    fn t21_approve_runs_then_once(cx: &mut TestAppContext) {
        let (asked, cx) = asked(cx);
        press(cx, "tab");
        activate(cx);
        cx.run_until_parked();
        ran_once_over(&asked, cx);
        assert!(trusted());
    }

    #[gpui::test]
    fn t21_mod_enter_twice_approves_and_runs_then_once(cx: &mut TestAppContext) {
        let (asked, cx) = asked(cx);
        // The second press comes while the approval is on its way.
        press_now(cx, "mod-enter mod-enter");
        cx.run_until_parked();
        ran_once_over(&asked, cx);
        assert!(trusted());
    }

    #[gpui::test]
    fn t21_esc_runs_then_once(cx: &mut TestAppContext) {
        let (asked, cx) = asked(cx);
        press(cx, "escape");
        cx.run_until_parked();
        ran_once_over(&asked, cx);
        assert!(!trusted());
    }
}
