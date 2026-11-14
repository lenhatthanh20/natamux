//! The Delete task confirm opens at once, before the read of the files that are not committed
//! ends (T-0307).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, TestAppContext};

use super::open_confirm_delete_task;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views::{self, Counted};
use crate::components::dialog::DialogLayer;

#[gpui::test]
fn the_delete_confirm_opens_before_the_read_ends(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (_root, cx) = cx.add_window_view(|_, cx| {
        DialogLayer::init(cx);
        Counted { draws: 0 }
    });
    // T-0137 has a worktree with 2 files that are not committed in the fake daemon.
    let task = dashboard
        .read_with(cx, |dashboard, _| dashboard.data.task("T-0137").cloned())
        .expect("T-0137");
    assert!(task.workspace.is_some());

    let dialog = cx.update(|window, cx| open_confirm_delete_task(&dashboard, &task, window, cx));
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
    dialog.read_with(cx, |dialog, _| {
        assert!(dialog.confirm.checking);
        assert_eq!(dialog.confirm.confirm_label.as_ref(), "Delete task");
    });

    cx.run_until_parked();
    dialog.read_with(cx, |dialog, _| {
        assert!(!dialog.confirm.checking);
        assert_eq!(dialog.confirm.confirm_label.as_ref(), "Delete anyway");
        assert!(dialog.confirm.description.contains("2 files"));
    });
}
