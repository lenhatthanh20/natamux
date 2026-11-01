//! The Progress tab of the panel: it gathers what the tab shows (`stages/progress_tab.rs` draws
//! the steps and the details card of the selected node), then Latest events below the stages.

use std::rc::Rc;

use gpui::AnyElement;
use natamux_proto::agents::activity::latest_line;
use natamux_proto::events::task_event::{TaskEvent, wait_reason};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::DetailsPanel;
use super::run::agent_of;
use crate::agents::agent_choices::agent_label;
use crate::app::clock::now_ms;
use crate::components::detail_section::DetailSection;
use crate::stages::card_rules::selected_stage;
use crate::stages::progress_tab::{ProgressInput, current_step, timeline};
use crate::stages::stage_view_rules::path_label;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::task_events_rules::{PathNames, describe_event};
use crate::tasks::task_time::{AgeStyle, format_age};
use crate::workspaces::wrap_up_rules::is_finished_state;

/// `latest`: the newest 8 accepted events.
const LATEST_EVENTS: usize = 8;

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
        let mut parts = Vec::new();
        if let Some(index) = current_step(&input)
            && self.shown.as_ref().is_some_and(|shown| !shown.scrolled)
        {
            // The steps are the first children; the step goes to the middle once drawn.
            self.center = Some(index);
            if let Some(shown) = self.shown.as_mut() {
                shown.scrolled = true;
            }
        }
        parts.extend(timeline(&input, &on, cx));
        parts.push(self.events_section(&input, cx).into_any_element());
        parts
    }

    fn events_section(&self, input: &ProgressInput, cx: &mut Context<Self>) -> impl IntoElement {
        // The names of the paths, so "Took Bug fix" names its path (`pathNames`).
        let names: PathNames = input
            .steps
            .iter()
            .filter_map(|step| match step {
                Step::Branch(branch) => Some(branch),
                _ => None,
            })
            .flat_map(|branch| &branch.paths)
            .map(|path| (path.id.clone(), path_label(path).to_owned()))
            .collect();
        let section = DetailSection::new(
            "latest",
            "Latest events",
            self.open_of("latest", false, cx),
            self.fold("latest", cx),
        );
        let latest: Vec<&TaskEvent> = input
            .events
            .iter()
            .rev()
            .filter(|event| event.kind != "agent.active")
            .take(LATEST_EVENTS)
            .collect();
        if latest.is_empty() {
            return section.child(
                Label::new("No events yet.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }
        section.children(latest.into_iter().map(|event| {
            let stage = event
                .payload
                .get("stage")
                .and_then(|value| value.as_str())
                .map(|id| {
                    let entry = input.progress.iter().find(|entry| entry.stage == id);
                    // A stage gone from the settings shows by its id.
                    let name = entry.map_or(id, |entry| entry.name.as_str());
                    format!(" \u{b7} {name}")
                })
                .unwrap_or_default();
            h_flex()
                .gap_2()
                .child(
                    Label::new(format_age(&event.at, input.now_ms, AgeStyle::Short, 0))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(format!(
                        "{}{stage}",
                        describe_event(&event.kind, &event.payload, &names)
                    ))
                    .size(LabelSize::Small),
                )
        }))
    }
}
