//! A step of the jobs spinner draws the status bar only: the title bar and the views beside it
//! are reused (AC2, test-spec 1.3).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;
use std::time::Duration;

use gpui::{Entity, TestAppContext, VisualTestContext, WeakEntity};
use natamux_proto::tasks::job::Job;
use ui::prelude::*;

use super::{StatusBarView, TitleBarView};
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views::{self, Counted, cached_style};
use crate::components::motion;

/// The window as Root lays it out: the two bars cached (`render_bars`), and the rest of the
/// window as one cached view.
struct Host {
    title: Entity<TitleBarView>,
    status: Entity<StatusBarView>,
    rest: Entity<Counted>,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .size_full()
            .child(self.title.clone().cached(cached_style()))
            .child(self.rest.clone().cached(cached_style()))
            .child(self.status.clone().cached(cached_style()))
    }
}

fn job() -> Job {
    Job {
        id: "job-1".into(),
        kind: "fetch".into(),
        label: "Fetching natamux".into(),
        task_id: None,
        started_at: "2026-10-08T09:00:00.000Z".into(),
    }
}

/// The draws of the title bar, the rest and the status bar.
fn draws(host: &Entity<Host>, cx: &mut VisualTestContext) -> (usize, usize, usize) {
    host.read_with(cx, |host, cx| {
        (
            host.title.read(cx).draws,
            host.rest.read(cx).draws,
            host.status.read(cx).draws,
        )
    })
}

#[gpui::test]
fn a_spinner_step_draws_only_the_status_bar(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), cx));
    let shown = dashboard.clone();
    let (host, cx) = cx.add_window_view(move |_, cx| Host {
        title: cx.new(|cx| TitleBarView::new(shown.clone(), cx)),
        status: cx.new(|cx| StatusBarView::new(WeakEntity::new_invalid(), shown.clone(), cx)),
        rest: cx.new(|_| Counted { draws: 0 }),
    });
    dashboard.update(cx, |dashboard, cx| {
        dashboard.data.jobs = vec![job()];
        cx.notify();
    });
    cx.update(|_, cx| motion::set_active(true, cx));
    cx.run_until_parked();
    let before = draws(&host, cx);
    assert!(before.2 > 0, "the status bar never drew");

    cx.executor().advance_clock(Duration::from_secs(1));
    cx.run_until_parked();

    let after = draws(&host, cx);
    assert!(after.2 > before.2, "the spinner did not step");
    assert_eq!(after.0, before.0, "the title bar drew for the spinner");
    assert_eq!(after.1, before.1, "the rest of the window drew for the spinner");
}
