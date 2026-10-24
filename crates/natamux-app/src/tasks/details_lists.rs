//! The list sections of the Details tab: Usage (`UsageSection.tsx`), Subtasks (`Subtasks.tsx`),
//! Tries (`CompareTries.tsx`), Details and Activity (`Timeline.tsx`).

use gpui::{AnyElement, FontWeight};
use natamux_proto::tasks::task::Task;
use ui::{ButtonLike, Tooltip, prelude::*};

use super::DetailsPanel;
use crate::app::clock::now_ms;
use crate::components::button_style::{action_button, quiet_button};
use crate::components::chips::StateChip;
use crate::components::detail_section::{DetailSection, field};
use crate::components::form::input_box;
use crate::tasks::details_commands::{OnCommand, PanelCommand};
use crate::tasks::details_rules::{details_fields, subtasks_aside};
use crate::tasks::table_rules::pull_request_number;
use crate::tasks::task_events_rules::{IGNORED_EVENT, NO_ACTIVITY, describe_line, timeline_lines};
use crate::tasks::task_time::{AgeStyle, format_age};
use crate::tasks::tries_rules::{can_keep, diff_stat_text, tries_of, try_group};
use crate::tasks::usage_rules::{NO_USAGE, usage_section};

fn small(text: impl Into<SharedString>) -> Label {
    Label::new(text).size(LabelSize::Small)
}

impl DetailsPanel {
    pub(super) fn lists(
        &self,
        task: &Task,
        on: &OnCommand,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut parts = Vec::new();
        parts.extend(self.usage(task, cx).map(IntoElement::into_any_element));
        if task.parent_id.is_none() {
            parts.push(self.subtasks(task, on, cx).into_any_element());
        }
        parts.extend(self.tries(task, on, cx).map(IntoElement::into_any_element));
        parts.push(self.facts(task, on, cx).into_any_element());
        parts.push(self.activity(cx).into_any_element());
        parts
    }

    fn usage(&self, task: &Task, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let usage = self.dashboard.read(cx).data.usage.get(&task.id).cloned();
        let text = usage_section(task.agent.as_deref(), usage.as_ref())?;
        let section = DetailSection::new(
            "usage",
            "Usage",
            self.open_of("usage", true, cx),
            self.fold("usage", cx),
        );
        let head = match &text.total {
            Some(total) => small(total.clone()),
            None => small(NO_USAGE).color(Color::Muted),
        };
        let rows = text.rows.into_iter().map(|row| {
            h_flex()
                .gap_2()
                .child(div().flex_1().min_w_0().child(small(row.stage).truncate()))
                .child(
                    div().flex_1().min_w_0().child(
                        small(format!("{} / {}", row.agent, row.model))
                            .color(Color::Muted)
                            .truncate(),
                    ),
                )
                .child(small(row.input).color(Color::Muted))
                .child(small(row.cache).color(Color::Muted))
                .child(small(row.output).color(Color::Muted))
                .child(
                    div()
                        .id(SharedString::from(format!(
                            "cost-{}-{}",
                            row.agent, row.model
                        )))
                        .when_some(row.cost_tip.clone(), |this, tip| {
                            this.tooltip(Tooltip::text(tip))
                        })
                        .child(small(row.cost)),
                )
        });
        Some(
            section.child(head).children(rows).child(
                Label::new(text.footer)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            ),
        )
    }

    fn subtasks(&self, task: &Task, on: &OnCommand, cx: &mut Context<Self>) -> impl IntoElement {
        let dashboard = self.dashboard.read(cx);
        let subtasks: Vec<Task> = dashboard
            .data
            .tasks
            .iter()
            .filter(|each| each.parent_id.as_deref() == Some(task.id.as_str()))
            .cloned()
            .collect();
        let busy = self.shown.as_ref().is_some_and(|shown| shown.busy);
        let empty = self.subtask.read(cx).text(cx).trim().is_empty();
        let rows = subtasks.iter().map(|subtask| {
            let on = on.clone();
            let id = subtask.id.clone();
            h_flex()
                .id(SharedString::from(format!("subtask-{}", subtask.id)))
                .gap_2()
                .rounded_sm()
                .cursor_pointer()
                .hover(|style| style.bg(cx.theme().colors().ghost_element_hover))
                .on_click(move |_, window, cx| on(PanelCommand::OpenTask(id.clone()), window, cx))
                .child(
                    small(subtask.id.clone())
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(small(subtask.title.clone()).truncate()),
                )
                .child(StateChip::new(subtask.state))
                .into_any_element()
        });
        let rows: Vec<AnyElement> = rows.collect();
        let add = on.clone();
        let open = self.open_of("subtasks", true, cx);
        DetailSection::new("subtasks", "Subtasks", open, self.fold("subtasks", cx))
            .aside(subtasks_aside(&subtasks))
            .children(rows)
            .child(
                h_flex()
                    .gap_2()
                    .child(div().flex_1().child(input_box(&self.subtask, cx)))
                    .child(
                        action_button("details-add-subtask", "Add")
                            .label_size(LabelSize::Small)
                            .disabled(busy || empty)
                            .on_click(move |_, window, cx| {
                                add(PanelCommand::AddSubtask, window, cx)
                            }),
                    ),
            )
    }

    fn tries(
        &self,
        task: &Task,
        on: &OnCommand,
        cx: &mut Context<Self>,
    ) -> Option<impl IntoElement> {
        let dashboard = self.dashboard.read(cx);
        let group = try_group(task)?;
        let tries: Vec<Task> = tries_of(group, &dashboard.data.tasks)
            .into_iter()
            .cloned()
            .collect();
        if tries.len() < 2 {
            return None;
        }
        let busy = self.shown.as_ref().is_some_and(|shown| shown.busy);
        let rows: Vec<AnyElement> = tries
            .iter()
            .map(|each| {
                let stat = diff_stat_text(dashboard.data.diff_stats.get(&each.id));
                let pr = each
                    .pr
                    .as_deref()
                    .map(pull_request_number)
                    .unwrap_or_default();
                let (open, keep) = (on.clone(), on.clone());
                let (id, kept) = (each.id.clone(), each.id.clone());
                let weight = if each.id == task.id {
                    FontWeight::BOLD
                } else {
                    FontWeight::NORMAL
                };
                h_flex()
                    .gap_2()
                    .child(
                        // The open try stays marked: bold, in the buffer font as all ids.
                        ButtonLike::new(SharedString::from(format!("try-{}", each.id)))
                            .toggle_state(each.id == task.id)
                            .on_click(move |_, window, cx| {
                                open(PanelCommand::OpenTask(id.clone()), window, cx)
                            })
                            .child(small(each.id.clone()).weight(weight).buffer_font(cx)),
                    )
                    .child(StateChip::new(each.state))
                    .child(div().flex_1().child(small(stat).color(Color::Muted)))
                    .child(small(pr).color(Color::Muted))
                    .when(can_keep(each), |this| {
                        this.child(
                            action_button(
                                SharedString::from(format!("keep-{}", each.id)),
                                "Keep this try",
                            )
                            .label_size(LabelSize::Small)
                            .disabled(busy)
                            .on_click(move |_, window, cx| {
                                keep(PanelCommand::KeepTry(kept.clone()), window, cx)
                            }),
                        )
                    })
                    .into_any_element()
            })
            .collect();
        let count = tries.len();
        Some(
            DetailSection::new(
                "tries",
                "Tries",
                self.open_of("tries", true, cx),
                self.fold("tries", cx),
            )
            .count(count)
            .children(rows),
        )
    }

    fn facts(&self, task: &Task, on: &OnCommand, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = details_fields(task, now_ms(), 0);
        let section = DetailSection::new(
            "details",
            "Details",
            self.open_of("details", false, cx),
            self.fold("details", cx),
        );
        section.children(fields.into_iter().map(|each| {
            let value: AnyElement = if each.label == "Part of" {
                let (on, id) = (on.clone(), each.text.clone());
                quiet_button("details-part-of", each.text.clone())
                    .label_size(LabelSize::Small)
                    .truncate(true)
                    .on_click(move |_, window, cx| {
                        on(PanelCommand::OpenTask(id.clone()), window, cx)
                    })
                    .into_any_element()
            } else {
                div()
                    .id(SharedString::from(format!("fact-{}", each.label)))
                    .when_some(each.tip.clone(), |this, tip| {
                        this.tooltip(Tooltip::text(tip))
                    })
                    .child(small(each.text.clone()))
                    .into_any_element()
            };
            field(each.label, value)
        }))
    }

    fn activity(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let section = DetailSection::new(
            "activity",
            "Activity",
            self.open_of("activity", true, cx),
            self.fold("activity", cx),
        );
        let events = self.shown.as_ref().and_then(|shown| shown.events.as_ref());
        let lines = match events {
            None => return section.child(small("Loading activity.").color(Color::Muted)),
            Some(Err(error)) => return section.child(small(error.clone()).color(Color::Error)),
            Some(Ok(events)) => timeline_lines(events),
        };
        if lines.is_empty() {
            return section.child(small(NO_ACTIVITY).color(Color::Muted));
        }
        let now = now_ms();
        section.children(lines.into_iter().map(|line| {
            let color = if line.accepted {
                Color::Default
            } else {
                Color::Muted
            };
            let ignored = if line.accepted { "" } else { IGNORED_EVENT };
            h_flex()
                .gap_2()
                .items_start()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(small(format!("{}{ignored}", describe_line(&line))).color(color)),
                )
                .child(
                    Label::new(format_age(&line.at, now, AgeStyle::Long, 0))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
        }))
    }
}
