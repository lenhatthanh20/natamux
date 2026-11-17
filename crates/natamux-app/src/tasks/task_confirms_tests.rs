//! The Delete task confirm opens at once, and its main button works before the read of the files
//! that are not committed ends (T-0307, T-0337).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, Entity, TestAppContext, VisualTestContext};
use natamux_proto::tasks::task::Task;

use super::open_confirm_delete_task;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views::{self, Counted};
use crate::components::dialog::{DialogLayer, DialogView};

/// A dashboard on the fake daemon, and its T-0137: a worktree with 2 files that are not committed.
fn setup(cx: &mut TestAppContext) -> (Entity<Dashboard>, Task, &mut VisualTestContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (_root, cx) = cx.add_window_view(|_, cx| {
        DialogLayer::init(cx);
        Counted { draws: 0 }
    });
    let task = dashboard
        .read_with(cx, |dashboard, _| dashboard.data.task("T-0137").cloned())
        .expect("T-0137");
    assert!(task.workspace.is_some());
    (dashboard, task, cx)
}

#[gpui::test]
fn the_delete_button_works_before_the_read_ends(cx: &mut TestAppContext) {
    let (dashboard, task, cx) = setup(cx);
    let dialog = cx.update(|window, cx| open_confirm_delete_task(&dashboard, &task, window, cx));
    assert!(cx.update(|_, cx| DialogLayer::is_open(cx)));
    dialog.read_with(cx, |dialog, _| {
        assert_eq!(dialog.confirm.confirm_label.as_ref(), "Delete task");
    });

    // A job that never ends holds the queue, so the delete never reaches the fake daemon, which
    // all tests of the process share: T-0137 stays for them.
    dashboard.update(cx, |dashboard, _| {
        dashboard
            .deletes
            .push(vec!["T-HOLD".into()], Vec::new(), false);
        dashboard.deletes.start_next();
    });

    // Before the read ends: the delete is queued at once, without force.
    dialog.update_in(cx, |dialog, window, cx| dialog.submit(window, cx));
    assert!(!cx.update(|_, cx| DialogLayer::is_open(cx)));
    dashboard.update(cx, |dashboard, _| {
        assert!(dashboard.data.task("T-0137").is_none());
        dashboard.deletes.finish();
        let job = dashboard.deletes.start_next().expect("the T-0137 job");
        assert_eq!(job.ids, vec!["T-0137".to_string()]);
        assert!(job.force.is_empty());
    });
}

#[gpui::test]
fn the_delete_confirm_names_the_files_after_the_read(cx: &mut TestAppContext) {
    let (dashboard, task, cx) = setup(cx);
    let dialog = cx.update(|window, cx| open_confirm_delete_task(&dashboard, &task, window, cx));

    cx.run_until_parked();
    dialog.read_with(cx, |dialog, _| {
        assert_eq!(dialog.confirm.confirm_label.as_ref(), "Delete anyway");
        assert!(dialog.confirm.description.contains("2 files"));
    });
}
