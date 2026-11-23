//! The CI approval dialog (Strict `approved-snapshot`): it shows the exact tree and files CI
//! gets, and Approve sends that tree id, so the daemon refuses the push if anything moved.

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::DetailsPanel;
use crate::app::dashboard::Dashboard;
use crate::components::confirm::Confirm;
use crate::stages::ci_approval_rules::{ask_description, ask_rows, pending_ci_asks};

/// The dialog lists at most this many files; the rest is one row.
const SHOWN_FILES: usize = 12;

impl DetailsPanel {
    pub(super) fn open_ci_approval(
        &mut self,
        task: &Task,
        stage: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let events: Vec<TaskEvent> = self
            .shown
            .as_ref()
            .and_then(|shown| shown.events.as_ref())
            .and_then(|read| read.as_ref().ok())
            .map(|stored| stored.iter().map(|each| each.event.clone()).collect())
            .unwrap_or_default();
        let asks = pending_ci_asks(&events);
        let Some(ask) = asks.into_iter().find(|ask| ask.stage == stage) else {
            return;
        };
        let rows = ask_rows(&ask, SHOWN_FILES)
            .into_iter()
            .map(|(id, file)| (id.into(), file.into()))
            .collect();
        let panel = cx.weak_entity();
        let (id, stage, tree) = (task.id.clone(), ask.stage.clone(), ask.tree.clone());
        Confirm::new(
            format!("Approve the CI push of {stage}?"),
            ask_description(&ask),
            "Approve and push",
            move |_, cx| {
                let (id, stage, tree) = (id.clone(), stage.clone(), tree.clone());
                let approve = move |d: &mut Dashboard, cx: &mut Context<Dashboard>| {
                    d.approve_ci(&id, &stage, &tree, cx)
                };
                panel
                    .update(cx, |panel, cx| panel.busy_call(cx, approve, |_, _| {}))
                    .ok();
            },
        )
        .rows(rows)
        .open(window, cx);
    }
}
