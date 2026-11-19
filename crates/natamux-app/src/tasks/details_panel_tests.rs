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

#[gpui::test]
fn a_task_gone_from_the_list_leaves_the_kept_details(cx: &mut TestAppContext) {
    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (panel_dashboard, workspace_dashboard) = (dashboard.clone(), dashboard.clone());
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        let workspace = cx.new(|cx| WorkspaceView::new(workspace_dashboard, cx));
        DetailsPanel::new(panel_dashboard, workspace, window, cx)
    });
    for id in ["T-0137", "T-0146"] {
        dashboard.update(cx, |dashboard, cx| dashboard.select(Some(id.into()), cx));
        panel.update(cx, |panel, cx| panel.sync(cx));
    }
    let kept = |cx: &mut gpui::VisualTestContext| {
        panel.read_with(cx, |panel, _| panel.cache.ids().contains(&"T-0137"))
    };
    assert!(kept(cx));

    let rest: Vec<_> = dashboard.read_with(cx, |dashboard, _| {
        let tasks = dashboard.data.tasks.iter();
        let rest = tasks.filter(|task| task.id != "T-0137");
        rest.map(|task| (**task).clone()).collect()
    });
    let event = natamux_client::DashboardEvent::Tasks(rest);
    dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
    cx.run_until_parked();
    assert!(!kept(cx));
}

#[gpui::test]
fn progress_is_worked_out_once_per_data_change(cx: &mut TestAppContext) {
    use super::run::COMPUTES;
    use crate::app::dashboard_data::Part;
    use natamux_proto::projects::repo::LoadedSettings;

    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (panel_dashboard, workspace_dashboard) = (dashboard.clone(), dashboard.clone());
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        let workspace = cx.new(|cx| WorkspaceView::new(workspace_dashboard, cx));
        DetailsPanel::new(panel_dashboard, workspace, window, cx)
    });
    dashboard.update(cx, |dashboard, cx| {
        dashboard.select(Some("T-0137".into()), cx)
    });
    panel.update(cx, |panel, cx| panel.sync(cx));
    panel.update(cx, |panel, _| {
        let shown = panel.shown.as_mut().expect("shown");
        shown.events = Some(Ok(Vec::new()));
        shown.settings = Some(Ok(LoadedSettings::default()));
    });
    let task = dashboard.read_with(cx, |dashboard, _| dashboard.data.task("T-0137").cloned());
    let task = task.expect("task");
    let computes = COMPUTES.with(|count| count.get());
    let compute = |cx: &mut gpui::VisualTestContext| {
        panel.update(cx, |panel, cx| panel.progress(&task, cx).is_some());
        COMPUTES.with(|count| count.get()) - computes
    };

    assert_eq!(compute(cx), 1);
    assert_eq!(compute(cx), 1);

    dashboard.update(cx, |dashboard, _| dashboard.data.gens.bump(Part::Tasks));
    assert_eq!(compute(cx), 2);

    panel.update(cx, |panel, _| panel.shown.as_mut().expect("shown").rev += 1);
    assert_eq!(compute(cx), 3);
}

#[test]
fn the_activity_lines_are_made_again_only_after_a_read() {
    use std::rc::Rc;

    use natamux_proto::events::task_event::{StoredEvent, TaskEvent};

    use super::Shown;
    use crate::tasks::task_events_rules::timeline_lines;

    let event = |seq: u64, kind: &str| StoredEvent {
        seq,
        accepted: true,
        event: TaskEvent {
            task_id: "T-0137".into(),
            source: "human".into(),
            kind: kind.into(),
            at: format!("2026-10-08T09:00:{seq:02}.000Z"),
            payload: Default::default(),
        },
    };
    let mut shown = Shown::default();
    let events = vec![
        event(1, "task.edited"),
        event(2, "agent.active"),
        event(3, "task.edited"),
        event(4, "task.readied"),
    ];
    let first = shown.activity_lines(&events);
    assert_eq!(*first, timeline_lines(&events));
    assert!(Rc::ptr_eq(&first, &shown.activity_lines(&events)));

    // A read that changed the events moves `rev`.
    shown.rev += 1;
    let events = vec![event(1, "task.created")];
    let again = shown.activity_lines(&events);
    assert!(!Rc::ptr_eq(&first, &again));
    assert_eq!(*again, timeline_lines(&events));
}

#[gpui::test]
fn only_a_change_the_view_shows_draws_the_panel(cx: &mut TestAppContext) {
    use std::cell::Cell;
    use std::rc::Rc;

    use natamux_client::DashboardEvent;
    use natamux_proto::agents::activity::{AgentActivity, TaskUsage};

    use super::follow::slices_of;
    use crate::app::view_rules::DetailsView;

    test_views::init(cx);
    let source = Arc::new(Source::FakeDaemon);
    let dashboard = cx.new(|cx| Dashboard::new(Ok(source), Default::default(), cx));
    cx.run_until_parked();
    let (panel_dashboard, workspace_dashboard) = (dashboard.clone(), dashboard.clone());
    let (panel, cx) = cx.add_window_view(move |window, cx| {
        let workspace = cx.new(|cx| WorkspaceView::new(workspace_dashboard, cx));
        DetailsPanel::new(panel_dashboard, workspace, window, cx)
    });
    let show = |view: DetailsView, cx: &mut gpui::VisualTestContext| {
        dashboard.update(cx, |dashboard, cx| {
            dashboard.select(Some("T-0137".into()), cx);
            dashboard.set_details_view(view, cx);
        });
        cx.run_until_parked();
        let now = dashboard.read_with(cx, |dashboard, _| dashboard.details_view_now());
        assert_eq!(now, view);
    };
    show(DetailsView::Details, cx);
    let draws = Rc::new(Cell::new(0));
    let seen = Rc::clone(&draws);
    cx.update(|_, cx| {
        cx.observe(&panel, move |_, _| seen.set(seen.get() + 1))
            .detach();
    });
    let usage = |id: &str, tokens: u64, cx: &mut gpui::VisualTestContext| {
        let mut all = dashboard.read_with(cx, |dashboard, _| dashboard.data.usage.clone());
        let used = TaskUsage {
            tokens,
            ..TaskUsage::default()
        };
        all.insert(id.into(), used);
        let event = DashboardEvent::Usage(all);
        dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
        cx.run_until_parked();
    };

    usage("T-0146", 123_456, cx);
    assert_eq!(draws.get(), 0, "the usage of another task drew the panel");
    usage("T-0137", 654_321, cx);
    assert!(
        draws.get() > 0,
        "the usage of the task shown did not draw the panel"
    );

    // Activity shows on Progress only.
    let slices = |cx: &mut gpui::VisualTestContext| {
        dashboard.read_with(cx, |dashboard, _| slices_of(dashboard))
    };
    let activity = |text: &str, cx: &mut gpui::VisualTestContext| {
        let mut all = dashboard.read_with(cx, |dashboard, _| dashboard.data.activity.clone());
        let line = AgentActivity {
            text: text.into(),
            at: "2026-10-22T10:00:00.000Z".into(),
        };
        all.insert("T-0137".into(), [("Terminal 1".to_owned(), line)].into());
        let event = DashboardEvent::Activity(all);
        dashboard.update(cx, |dashboard, cx| dashboard.take_event(event, cx));
        cx.run_until_parked();
    };
    let before = slices(cx);
    activity("Edit a.rs", cx);
    assert_eq!(slices(cx), before);
    show(DetailsView::Progress, cx);
    let before = slices(cx);
    activity("Edit b.rs", cx);
    assert_ne!(slices(cx), before);
}
