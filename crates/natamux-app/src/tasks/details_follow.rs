//! How the panel follows the dashboard. Root caches the panel, so a change of what it reads draws
//! it again from here.

use gpui::{Entity, Subscription};
use natamux_proto::agents::activity::{TaskUsage, latest_line};
use natamux_proto::workspaces::workspace::DiffStat;
use ui::prelude::*;

use super::DetailsPanel;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::{Part, Seen};
use crate::app::view_rules::DetailsView;
use crate::tasks::tries_rules::{tries_of, try_group};

/// What the view shown reads of the parts that change while agents work. They change many times
/// a second, mostly for other tasks or for what the view shown does not read.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Slices {
    usage: Option<TaskUsage>,
    /// The newest line: all that Progress shows of the activity.
    activity: Option<String>,
    /// The diff stats of the task's tries, for the Tries section of Details.
    tries: Vec<(String, Option<DiffStat>)>,
}

/// Usage shows on Details and Progress, activity on Progress, the tries' stats on Details. The
/// other views read none of them.
pub(super) fn slices_of(dashboard: &Dashboard) -> Slices {
    let Some(task) = dashboard.details_task() else {
        return Slices::default();
    };
    let view = dashboard.details_view_now();
    let (details, progress) = (view == DetailsView::Details, view == DetailsView::Progress);
    let data = &dashboard.data;
    let usage = data.usage.get(&task.id).filter(|_| details || progress);
    let activity = data.activity.get(&task.id).filter(|_| progress);
    let tries = match try_group(task).filter(|_| details) {
        Some(group) => tries_of(group, &data.tasks)
            .into_iter()
            .map(|each| (each.id.clone(), data.diff_stats.get(&each.id).copied()))
            .collect(),
        None => Vec::new(),
    };
    Slices {
        usage: usage.cloned(),
        activity: activity.and_then(latest_line).map(str::to_owned),
        tries,
    }
}

impl DetailsPanel {
    /// The task shown follows the open or the selected task (View, Tasks).
    pub(super) fn follow_dashboard(
        dashboard: &Entity<Dashboard>,
        cx: &mut Context<Self>,
    ) -> Subscription {
        let mut seen = Seen::default();
        let mut last = Slices::default();
        cx.observe(dashboard, move |panel, dashboard, cx| {
            let gens = dashboard.read(cx).data.gens;
            let changed = seen.take(&gens);
            let moved = changed.any(&[Part::Tasks, Part::View]);
            if moved {
                panel.sync(cx);
            }
            let parts = [
                Part::Tasks,
                Part::View,
                Part::Usage,
                Part::Activity,
                Part::DiffStats,
            ];
            if !changed.any(&parts) {
                return;
            }
            // Kept on every change, also while the view hides them: a draw reads the newest.
            let now = slices_of(dashboard.read(cx));
            let read_changed = now != last;
            last = now;
            if moved || read_changed {
                cx.notify();
            }
        })
    }
}
