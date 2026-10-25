//! The panel's calls to the daemon. Every action button is off while one runs (`busy`), so a
//! double click sends one request; the stage settings and Resume have their own busy marks.

use natamux_client::ClientError;
use natamux_proto::stages::actions::{StageAction, StageActionInput};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::DetailsPanel;
use super::run::failure_of;
use crate::app::dashboard::Dashboard;
use crate::tasks::details_commands::StageToggle;

type Written<T> = gpui::Task<Result<T, ClientError>>;

impl DetailsPanel {
    fn busy(&self) -> bool {
        self.shown.as_ref().is_some_and(|shown| shown.busy)
    }

    /// Runs one call unless one is on its way, keeps the buttons off until it answers, then runs
    /// `done` when it worked.
    pub(super) fn busy_call<T: 'static>(
        &mut self,
        cx: &mut Context<Self>,
        call: impl FnOnce(&mut Dashboard, &mut Context<Dashboard>) -> Written<T>,
        done: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        if self.busy() {
            return;
        }
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        shown.busy = true;
        let id = shown.task_id.clone();
        let written = self.dashboard.update(cx, call);
        cx.notify();
        cx.spawn(async move |panel, cx| {
            let worked = written.await.is_ok();
            panel
                .update(cx, |panel, cx| {
                    if let Some(shown) = panel.shown.as_mut().filter(|s| s.task_id == id) {
                        shown.busy = false;
                    }
                    if worked {
                        done(panel, cx);
                    }
                    cx.notify();
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn act(
        &mut self,
        task: &Task,
        action: StageAction,
        stage: String,
        cx: &mut Context<Self>,
    ) {
        let input = StageActionInput {
            action,
            stage: Some(stage),
            to: None,
            reason: None,
        };
        let id = task.id.clone();
        self.busy_call(
            cx,
            move |dashboard, cx| dashboard.stage_action(&id, input, failure_of(action), cx),
            |_, _| {},
        );
    }

    /// Start agent, Restart agent and the card's picker: the workspace opens on the agent.
    pub(super) fn start_agent(&mut self, task: &Task, agent: String, cx: &mut Context<Self>) {
        let (id, opened) = (task.id.clone(), task.id.clone());
        self.busy_call(
            cx,
            move |dashboard, cx| dashboard.start_agent(&id, &agent, cx),
            move |panel, cx| panel.dashboard.update(cx, |d, cx| d.open(&opened, cx)),
        );
    }

    pub(super) fn toggle(
        &mut self,
        task: &Task,
        kind: StageToggle,
        stage: String,
        on: bool,
        cx: &mut Context<Self>,
    ) {
        let key = format!("{kind:?}:{stage}");
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        if !shown.toggling.insert(key.clone()) {
            return;
        }
        let id = task.id.clone();
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.set_stage_toggle(&id, kind, &stage, on, cx)
        });
        cx.spawn(async move |panel, cx| {
            written.await.ok();
            panel
                .update(cx, |panel, cx| {
                    if let Some(shown) = panel.shown.as_mut() {
                        shown.toggling.remove(&key);
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
    }

    pub(super) fn resume(&mut self, task: &Task, cx: &mut Context<Self>) {
        let Some(shown) = self.shown.as_mut().filter(|shown| !shown.resuming) else {
            return;
        };
        shown.resuming = true;
        let id = task.id.clone();
        let written = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.send_event(&id, "agent.resumed", cx)
        });
        cx.spawn(async move |panel, cx| {
            written.await.ok();
            panel
                .update(cx, |panel, cx| {
                    if let Some(shown) = panel.shown.as_mut() {
                        shown.resuming = false;
                        cx.notify();
                    }
                })
                .ok();
        })
        .detach();
    }
}
