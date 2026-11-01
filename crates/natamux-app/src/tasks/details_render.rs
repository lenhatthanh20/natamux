//! The Progress tab of the panel: it gathers what the tab shows (`stages/progress_tab.rs` draws
//! the steps and the details card of the selected node).

use std::rc::Rc;

use gpui::AnyElement;
use natamux_proto::agents::activity::latest_line;
use natamux_proto::events::task_event::{TaskEvent, wait_reason};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::DetailsPanel;
use super::run::agent_of;
use crate::agents::agent_choices::agent_label;
use crate::app::clock::now_ms;
use crate::stages::card_rules::selected_stage;
use crate::stages::progress_tab::{ProgressInput, current_step, timeline};
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::workspaces::wrap_up_rules::is_finished_state;

impl DetailsPanel {
    pub(super) fn on_command(&self, cx: &mut Context<Self>) -> OnCommand {
        let panel = cx.weak_entity();
        Rc::new(
            move |command: PanelCommand, window: &mut Window, cx: &mut App| {
                panel
                    .update(cx, |panel, cx| panel.run(command, window, cx))
                    .ok();
            },
        )
    }

    /// A focus handle for each node, kept while the task shows: Enter and Space select it.
    fn make_card_focus(&mut self, task: &Task, cx: &mut Context<Self>) {
        let Some((_, progress)) = self.progress(task) else {
            return;
        };
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        for entry in &progress {
            shown
                .card_focus
                .entry(entry.stage.clone())
                .or_insert_with(|| cx.focus_handle().tab_stop(true));
        }
    }

    fn progress_input(&self, task: &Task, cx: &App) -> Option<ProgressInput> {
        let shown = self.shown.as_ref()?;
        let (steps, progress) = self.progress(task)?;
        let stored = shown.events.as_ref()?.as_ref().ok()?;
        let events: Vec<TaskEvent> = stored
            .iter()
            .filter(|s| s.accepted)
            .map(|s| s.event.clone())
            .collect();
        let dashboard = self.dashboard.read(cx);
        let beside = dashboard.open_task.as_deref() == Some(task.id.as_str());
        let selected = selected_stage(shown.picked.as_deref(), &progress);
        Some(ProgressInput {
            steps,
            progress,
            files: shown.files.clone(),
            usage: dashboard.data.usage.get(&task.id).cloned(),
            activity: dashboard
                .data
                .activity
                .get(&task.id)
                .and_then(|lines| latest_line(lines).map(str::to_owned)),
            wait: wait_reason(Some(stored)),
            now_ms: now_ms(),
            busy: shown.busy,
            resuming: shown.resuming,
            toggling: shown.toggling.clone(),
            selected,
            task_agent: agent_of(task),
            open_lanes: shown.open_lanes.clone(),
            open_evidence: shown.open_evidence.clone(),
            card_focus: shown.card_focus.clone(),
            agent_label: agent_label(Some(
                shown
                    .agent
                    .clone()
                    .unwrap_or_else(|| agent_of(task))
                    .as_str(),
            )),
            can_answer: beside && task.workspace.is_some(),
            can_act: !is_finished_state(task.state),
            events,
            task: task.clone(),
        })
    }

    pub(super) fn render_progress(
        &mut self,
        task: &Task,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let reading = |text: String| {
            vec![
                div()
                    .p_3()
                    .child(Label::new(text).color(Color::Muted))
                    .into_any_element(),
            ]
        };
        let Some(shown) = self.shown.as_ref() else {
            return Vec::new();
        };
        // A read that failed is an alert line, as the web's role=alert.
        let failed = |error: &String| {
            vec![
                div()
                    .p_3()
                    .child(Label::new(error.clone()).color(Color::Error))
                    .into_any_element(),
            ]
        };
        if let Some(Err(error)) = &shown.settings {
            return failed(error);
        }
        if let Some(Err(error)) = &shown.events {
            return failed(error);
        }
        self.make_card_focus(task, cx);
        let Some(input) = self.progress_input(task, cx) else {
            return reading("Reading the stages\u{2026}".into());
        };
        let on = self.on_command(cx);
        if let Some(index) = current_step(&input)
            && self.shown.as_ref().is_some_and(|shown| !shown.scrolled)
        {
            // The steps are the first children; the step goes to the middle once drawn.
            self.center = Some(index);
            if let Some(shown) = self.shown.as_mut() {
                shown.scrolled = true;
            }
        }
        timeline(&input, &on, cx)
    }
}
