//! The Progress tab of the panel: it gathers what the tab shows (`stages/progress_tab.rs` draws
//! the steps), then the overview card and the sections below the stages.

use std::rc::Rc;

use gpui::{AnyElement, FontWeight};
use natamux_proto::agents::activity::latest_line;
use natamux_proto::events::task_event::{TaskEvent, wait_reason};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::DetailsPanel;
use super::run::agent_of;
use crate::agents::agent_choices::agent_label;
use crate::app::clock::now_ms;
use crate::app::links::open_link;
use crate::components::button_style::{action_button, quiet_button};
use crate::components::detail_section::{DetailSection, field, field_fit};
use crate::stages::overview_rules::{NO_STAGE_STARTED, strip_figures, strip_head, time_bars};
use crate::stages::progress_tab::{ProgressInput, counts, current_step, timeline};
use crate::stages::stage_view_rules::{path_label, sent_back_count};
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::task_events_rules::{PathNames, describe_event};
use crate::tasks::task_rules::context_text;
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

    /// A focus handle for the fold row of each finished or upcoming node, kept while the task
    /// shows. The current node is always open and a not-taken one does not fold.
    fn make_card_focus(&mut self, task: &Task, cx: &mut Context<Self>) {
        let Some((_, progress)) = self.progress(task) else {
            return;
        };
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        let foldable = progress
            .iter()
            .filter(|entry| !matches!(entry.status, StageStatus::Current | StageStatus::NotTaken));
        for entry in foldable {
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
            open_cards: shown.open_cards.clone(),
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
        let mut parts = vec![self.overview(&input, cx)];
        if let Some(index) = current_step(&input)
            && self.shown.as_ref().is_some_and(|shown| !shown.scrolled)
        {
            // The overview is the first child; the step goes to the middle once drawn.
            self.center = Some(index + 1);
            if let Some(shown) = self.shown.as_mut() {
                shown.scrolled = true;
            }
        }
        parts.extend(timeline(&input, &on, cx));
        parts.push(self.context_section(&input, cx).into_any_element());
        parts.push(self.time_section(&input, cx).into_any_element());
        parts.push(self.events_section(&input, cx).into_any_element());
        parts
    }

    /// The stat strip on top: the figures in one row, and above them how an ended task ended.
    fn overview(&self, input: &ProgressInput, cx: &App) -> AnyElement {
        let (done, total) = counts(&input.progress);
        let figures = strip_figures(
            &input.task,
            done,
            total,
            sent_back_count(&input.events),
            input.usage.as_ref().and_then(|usage| usage.cost_usd),
            input.now_ms,
        );
        let head = strip_head(&input.task, &input.progress, input.now_ms);
        let color = if input.task.state == TaskState::Done {
            Color::Success
        } else {
            Color::Muted
        };
        let colors = cx.theme().colors();
        let head_row = head.map(|text| {
            div()
                .px_3()
                .py_1()
                .border_b_1()
                .border_color(colors.border_variant)
                .child(
                    Label::new(text)
                        .size(LabelSize::Small)
                        .weight(FontWeight::SEMIBOLD)
                        .color(color),
                )
        });
        let cells = figures.into_iter().enumerate().map(|(index, figure)| {
            v_flex()
                .id(SharedString::from(format!("figure-{index}")))
                .flex_1()
                .px_3()
                .py_1()
                .when(index > 0, |this| {
                    this.border_l_1().border_color(colors.border_variant)
                })
                .tooltip(ui::Tooltip::text(figure.tip))
                .child(Label::new(figure.value).weight(FontWeight::SEMIBOLD))
                .child(
                    Label::new(figure.label)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
        });
        div()
            .px_3()
            .pt_3()
            .pb_1()
            .child(
                v_flex()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.elevated_surface_background)
                    .children(head_row)
                    .child(h_flex().children(cells)),
            )
            .into_any_element()
    }

    fn context_section(&self, input: &ProgressInput, cx: &mut Context<Self>) -> impl IntoElement {
        let task = &input.task;
        let context = task
            .context
            .as_ref()
            .map_or_else(|| "The task notes".to_owned(), context_text);
        let settings = self
            .shown
            .as_ref()
            .and_then(|shown| shown.settings.as_ref())
            .and_then(|settings| settings.as_ref().ok());
        let can_create = input.can_act
            && settings.is_some_and(|loaded| {
                let tracker = &loaded.settings.tracker;
                tracker.kind == "github" && tracker.create_issue != "never"
            });
        let creates = task.issue.is_none() && can_create;
        let issue: AnyElement = match &task.issue {
            Some(issue) => {
                let url = issue.url.clone();
                quiet_button("issue-link", issue.key.clone())
                    .label_size(LabelSize::Small)
                    .truncate(true)
                    .on_click(move |_, _, cx| open_link(&url, cx))
                    .into_any_element()
            }
            None if can_create => {
                let on = self.on_command(cx);
                action_button("create-issue", "Create issue")
                    .size(ButtonSize::Compact)
                    .label_size(LabelSize::Small)
                    .disabled(self.shown.as_ref().is_some_and(|shown| shown.busy))
                    .on_click(move |_, window, cx| on(PanelCommand::CreateIssue, window, cx))
                    .into_any_element()
            }
            None => Label::new("None").size(LabelSize::Small).into_any_element(),
        };
        let pr: AnyElement = match &task.pr {
            Some(url) => {
                let link = url.clone();
                quiet_button(
                    "context-pr",
                    url.trim_start_matches("https://github.com/").to_owned(),
                )
                .label_size(LabelSize::Small)
                .truncate(true)
                .on_click(move |_, _, cx| open_link(&link, cx))
                .into_any_element()
            }
            None => Label::new("Opens in Release")
                .size(LabelSize::Small)
                .into_any_element(),
        };
        DetailSection::new(
            "context",
            "Context and issue",
            self.open_of("context", true, cx),
            self.fold("context", cx),
        )
        .child(field("Context", Label::new(context).size(LabelSize::Small)))
        .child(if creates {
            field_fit("Issue", issue).into_any_element()
        } else {
            field("Issue", issue).into_any_element()
        })
        .child(field("Pull request", pr))
    }

    fn time_section(&self, input: &ProgressInput, cx: &mut Context<Self>) -> impl IntoElement {
        let bars = time_bars(&input.progress, input.now_ms);
        let fill = cx.theme().colors().text_accent;
        let section = DetailSection::new(
            "time",
            "Time per stage",
            self.open_of("time", false, cx),
            self.fold("time", cx),
        );
        if bars.is_empty() {
            return section.child(
                Label::new(NO_STAGE_STARTED)
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            );
        }
        section.children(bars.into_iter().map(|bar| {
            h_flex()
                .gap_2()
                .child(
                    div()
                        .w(rems_from_px(96.0_f32))
                        .child(Label::new(bar.name).size(LabelSize::Small).truncate()),
                )
                .child(
                    div().flex_1().h(rems_from_px(6.0_f32)).child(
                        div()
                            .h_full()
                            .rounded_sm()
                            .bg(fill)
                            .w(relative((bar.width_percent / 100.) as f32)),
                    ),
                )
                .child(
                    Label::new(bar.time)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
        }))
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
