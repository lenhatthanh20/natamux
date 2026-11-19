//! The followers of the dashboard are told by its signal, so a change of the feed draws no
//! window frame by itself.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use gpui::{Entity, Subscription, TestAppContext};
use natamux_client::DashboardEvent;
use natamux_proto::agents::activity::TaskUsage;
use ui::prelude::*;

use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;
use crate::components::toast_rules::ToastTone;

/// Follows the dashboard both ways, to count each.
struct Follower {
    _subscriptions: [Subscription; 2],
}

type Count = Rc<Cell<usize>>;

/// A follower, with the notifies of the dashboard itself, which draw the window, and the calls
/// it got from the signal.
fn follower(
    dashboard: &Entity<Dashboard>,
    cx: &mut TestAppContext,
) -> (Entity<Follower>, Count, Count) {
    let (notified, told) = (Count::default(), Count::default());
    let (count_notified, count_told) = (notified.clone(), told.clone());
    let follower = cx.new(|cx| Follower {
        _subscriptions: [
            cx.observe(dashboard, move |_, _, _| {
                count_notified.set(count_notified.get() + 1);
            }),
            Dashboard::observe(dashboard, cx, move |_, _, _| {
                count_told.set(count_told.get() + 1);
            }),
        ],
    });
    cx.run_until_parked();
    (follower, notified, told)
}

#[gpui::test]
fn the_feed_and_a_toast_tell_the_followers_without_a_notify(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (_follower, notified, told) = follower(&dashboard, cx);

    let usage = TaskUsage {
        tokens: 99,
        ..TaskUsage::default()
    };
    let event = DashboardEvent::Usage([("T-0146".to_string(), usage)].into());
    dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
    cx.run_until_parked();
    assert_eq!((notified.get(), told.get()), (0, 1));

    dashboard.update(cx, |dashboard, cx| {
        dashboard.toast("Saved.", ToastTone::Info, None, cx)
    });
    cx.run_until_parked();
    assert_eq!((notified.get(), told.get()), (0, 2));

    // Root reads the dashboard's own state when it draws: this notify draws the window, and
    // the followers hear of it too.
    dashboard.update(cx, |dashboard, cx| dashboard.select(None, cx));
    cx.run_until_parked();
    assert_eq!((notified.get(), told.get()), (1, 3));
}

fn rust_files(folder: &Path, out: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

/// An observer of the dashboard itself would miss every change of the feed.
#[test]
fn every_follower_uses_the_signal() {
    let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut files = Vec::new();
    rust_files(&src, &mut files);
    let calls = [
        "cx.observe(&dashboard,",
        "cx.observe(dashboard,",
        "cx.observe_in(&dashboard,",
        "cx.observe_in(dashboard,",
    ];
    let found: Vec<_> = files
        .iter()
        .filter(|path| !path.ends_with("app/dashboard_signal_tests.rs"))
        .filter(|path| {
            let text = std::fs::read_to_string(path).unwrap();
            calls.iter().any(|call| text.contains(call))
        })
        .collect();
    assert!(
        found.is_empty(),
        "use Dashboard::observe instead: {found:?}"
    );
}
