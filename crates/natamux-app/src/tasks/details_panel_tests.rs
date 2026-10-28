//! A switch back to a task seen lately draws what the panel kept and reads again (T-0223, P5).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::sync::Arc;

use gpui::{AppContext as _, TestAppContext};

use super::DetailsPanel;
use crate::app::dashboard::Dashboard;
use crate::app::source::Source;
use crate::app::test_views;
use crate::workspaces::workspace_view::WorkspaceView;

#[gpui::test]
fn a_switch_back_keeps_the_read_events_and_reads_again(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (panel_dashboard, workspace_dashboard) = (dashboard.clone(), dashboard.clone());
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        let workspace = cx.new(|cx| WorkspaceView::new(workspace_dashboard, cx));
        DetailsPanel::new(panel_dashboard, workspace, window, cx)
    });
    let pick = |id: &str, cx: &mut gpui::VisualTestContext| {
        dashboard.update(cx, |dashboard, cx| dashboard.select(Some(id.into()), cx));
        panel.update(cx, |panel, cx| panel.sync(cx));
    };

    pick("T-0137", cx);
    panel.update(cx, |panel, _| {
        let shown = panel.shown.as_mut().expect("shown");
        shown.events = Some(Ok(Vec::new()));
        shown.read = true;
    });
    pick("T-0146", cx);
    pick("T-0137", cx);

    // Kept for the first frame; the draw that follows the switch reads everything again.
    cx.run_until_parked();
    panel.read_with(cx, |panel, _| {
        let shown = panel.shown.as_ref().expect("shown");
        assert_eq!(shown.task_id, "T-0137");
        assert!(shown.events.is_some());
        assert!(shown.read);
    });
}
