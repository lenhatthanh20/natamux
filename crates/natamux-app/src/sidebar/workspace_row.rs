//! The small pieces of the sidebar rows: the terminal row inside an open card, the inline rename
//! field, the title, the age and the reason of a card (drawn in `task_card.rs`), and the tooltips.

use gpui::{AnyElement, ClickEvent, FontWeight, Stateful};
use natamux_proto::agents::activity::latest_line;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::blur_bar::{BlurBar, private_text};
use super::sidebar::{agent_mark, lucide, mono, small};
use super::task_card::{GLYPH_COLUMN, HEAD_GAP};
use super::workspace_tree::{OFFSET_MINUTES, Snapshot, WorkspaceTree};
use crate::agents::agent_choices::agent_label;
use crate::app::dashboard_data::DashboardData;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::sidebar::card_rules::waiting_of;
use crate::sidebar::comet::comet_of;
use crate::sidebar::shell_rows::{ShellRow, shell_row_name, shell_tag};
use crate::sidebar::sidebar_text::{
    RowTip, card_line, pull_request_summary, row_tooltip, short_reason, step_text,
};
use crate::sidebar::sidebar_tree::{RowTier, row_tier, subtask_count};
use crate::sidebar::workspace_groups::DEFAULT_REPO_ID;
use crate::tasks::task_time::{AgeStyle, format_age, format_exact};

impl WorkspaceTree {
    /// The look every focusable row shares, as Zed's `ListItem`: hover, the open task, and the
    /// focus ring.
    pub(super) fn row_frame(&self, key: &str, current: bool, snap: &Snapshot, cx: &App) -> Div {
        let colors = cx.theme().colors();
        let focused = snap.focused && snap.stop.as_deref() == Some(key);
        div()
            .w_full()
            .rounded_sm()
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .when(current, |row| row.bg(colors.ghost_element_selected))
            .border_1()
            .border_color(if focused {
                colors.border_focused
            } else {
                gpui::transparent_black()
            })
    }

    pub(super) fn terminal_row(
        &self,
        task_id: &str,
        row: &ShellRow,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("agent:{task_id}:{}", row.key);
        let blurred = self
            .dashboard
            .read(cx)
            .data
            .task(task_id)
            .map(|task| task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID).to_owned())
            .is_some_and(|repo| snap.blurred.contains(&repo));
        let glyph: AnyElement = if row.plain {
            div()
                .flex_none()
                .size(rems_from_px(14.0_f32))
                .flex()
                .items_center()
                .justify_center()
                .child(lucide("square-terminal").color(Color::Muted))
                .into_any_element()
        } else {
            StateGlyph::new(SharedString::from(format!("{key}-glyph")), row.state)
                .size(ShapeSize::Terminal)
                .into_any_element()
        };
        let activity = row
            .shell
            .as_ref()
            .and_then(|shell| snap.terminal_activity.get(task_id)?.get(shell))
            .map(|text| {
                private_text(
                    blurred,
                    || BlurBar::new(text.clone(), 12.0),
                    || small(text.clone()).truncate().into_any_element(),
                )
            });
        let tag = shell_tag(row).map(|tag| {
            div()
                .id(SharedString::from(format!("{key}-tag")))
                .flex_none()
                .child(private_text(
                    blurred,
                    || BlurBar::new(tag.clone(), 11.0).mono(),
                    || mono(tag.clone(), cx).into_any_element(),
                ))
                .tooltip(Tooltip::text(format!(
                    "Terminal {}",
                    row.shell.as_deref().unwrap_or_default()
                )))
        });
        let agent = row
            .agent
            .as_deref()
            .map(|agent| agent_mark(SharedString::from(format!("{key}-agent")).into(), agent));
        let label = private_text(
            blurred,
            || BlurBar::new(row.label.clone(), 12.0).weight(FontWeight::MEDIUM),
            || {
                Label::new(row.label.clone())
                    .size(LabelSize::Custom(rems_from_px(12_f32)))
                    .weight(FontWeight::MEDIUM)
                    .color(if row.plain {
                        Color::Muted
                    } else {
                        Color::Default
                    })
                    .single_line()
                    .into_any_element()
            },
        );
        // The dot sits in the task glyph's column, so the agent icon starts at the title's edge.
        let glyph = div()
            .flex_none()
            .w(rems_from_px(GLYPH_COLUMN))
            .flex()
            .items_center()
            .justify_center()
            .debug_selector({
                let key = key.clone();
                move || format!("{key}-glyph")
            })
            .child(glyph);
        let content = h_flex()
            .debug_selector({
                let key = key.clone();
                move || format!("{key}-content")
            })
            .min_w_0()
            .flex_1()
            .gap_2()
            .children(agent)
            .child(label)
            .child(div().min_w_0().flex_1().children(activity))
            .children(tag);
        let open = key.clone();
        self.row_frame(&key, false, snap, cx)
            .id(SharedString::from(key.clone()))
            .h(rems_from_px(22.0_f32))
            .px_2()
            .flex()
            .items_center()
            .gap(rems_from_px(HEAD_GAP))
            .child(glyph)
            .child(content)
            .tooltip(Tooltip::text(shell_row_name(row)))
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(open.clone(), window, cx);
                tree.activate(&open, cx);
            }))
            .into_any_element()
    }

    /// The field that takes the title's place while renaming: Enter saves, Esc cancels (the tree
    /// handles `Back`), and leaving it saves.
    pub(super) fn rename_field(
        &self,
        editor: gpui::Entity<editor::Editor>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        div()
            .h(rems_from_px(24.0_f32))
            .w_full()
            .px(rems_from_px(6.0_f32))
            .flex()
            .items_center()
            .rounded_sm()
            .border_1()
            .border_color(colors.border_focused)
            .bg(colors.editor_background)
            .capture_key_down(cx.listener(|tree, event: &gpui::KeyDownEvent, window, cx| {
                if event.keystroke.key == "enter" {
                    tree.finish_rename(true, true, window, cx);
                    cx.stop_propagation();
                }
            }))
            .child(editor)
            .into_any_element()
    }
}

/// The id (only with Task ids in the sidebar, T-0240), then the title in the look of its tier:
/// bold when unseen or waiting, muted when finished. The id never shrinks; only the title
/// truncates.
pub(super) fn title_label(
    task: &Task,
    unseen: bool,
    blurred: bool,
    show_id: bool,
    cx: &App,
) -> AnyElement {
    let tier = row_tier(task.state);
    let weight = match tier {
        RowTier::Quiet => FontWeight::NORMAL,
        _ if unseen || tier == RowTier::Attention => FontWeight::SEMIBOLD,
        _ => FontWeight::MEDIUM,
    };
    let size = if tier == RowTier::Quiet {
        12_f32
    } else {
        13_f32
    };
    let finished = matches!(task.state, TaskState::Done | TaskState::Cancelled);
    let plain = || {
        let title = Label::new(task.title.clone())
            .size(LabelSize::Custom(rems_from_px(size)))
            .weight(weight)
            .color(if finished {
                Color::Muted
            } else {
                Color::Default
            })
            .truncate();
        if task.state == TaskState::Cancelled {
            title.strikethrough().into_any_element()
        } else {
            title.into_any_element()
        }
    };
    let title = private_text(
        blurred,
        || BlurBar::new(task.title.clone(), size).weight(weight),
        plain,
    );
    h_flex()
        .min_w_0()
        .gap_1p5()
        .when(show_id, |line| {
            line.child(
                div().flex_none().child(
                    Label::new(task.id.clone())
                        .size(LabelSize::Custom(rems_from_px(size)))
                        .color(Color::Muted)
                        .buffer_font(cx)
                        .single_line(),
                ),
            )
        })
        .child(div().min_w_0().child(title))
        .into_any_element()
}

/// The age of the last change.
pub(super) fn age_label(task: &Task, snap: &Snapshot) -> Stateful<Div> {
    let age = format_age(
        &task.updated_at,
        snap.now_ms,
        AgeStyle::Short,
        OFFSET_MINUTES,
    );
    div()
        .id(SharedString::from(format!("age-{}", task.id)))
        .flex_none()
        .child(
            div()
                .debug_selector(|| format!("age-{}", task.id))
                .child(small(age)),
        )
        .tooltip(Tooltip::text(format_exact(
            &task.updated_at,
            OFFSET_MINUTES,
        )))
}

/// The right slot of a card: the short reason in the state colour on an attention card (always
/// shown), else the age.
pub(super) fn right_slot(task: &Task, snap: &Snapshot, cx: &App) -> AnyElement {
    let Some(label) = snap.labels.get(&task.id) else {
        return age_label(task, snap).into_any_element();
    };
    div()
        .flex_none()
        .child(
            Label::new(label.clone())
                .size(LabelSize::Custom(rems_from_px(12_f32)))
                .color(Color::Custom(state_colors(task.state, cx).fg))
                .single_line(),
        )
        .into_any_element()
}

impl Snapshot {
    /// Fills what the card of `task` says outside its line: the tooltip, the short reason, the
    /// subtask count, the waiting box and what each terminal's agent does.
    pub(super) fn add_task(
        &mut self,
        data: &DashboardData,
        task: &Task,
        pinned_row: bool,
        blurred: &[String],
    ) {
        let id = &task.id;
        let events = data.events.get(id).map(|(_, events)| events.as_slice());
        let activity = data.activity.get(id);
        let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
        let subtasks =
            subtask_count(id, &data.tasks).map(|count| format!("{}/{}", count.done, count.total));
        let project = pinned_row
            .then(|| data.repos.iter().find(|known| known.id == repo))
            .flatten()
            .map(|known| known.name.as_str());
        let texts = row_texts(
            task,
            project,
            events,
            activity.and_then(latest_line),
            subtasks.as_deref(),
            blurred.iter().any(|hidden| hidden == repo),
        );
        self.tips.insert(id.clone(), texts.tip);
        if let Some(subtasks) = subtasks {
            self.subtasks.insert(id.clone(), subtasks);
        }
        if let Some(waiting) = waiting_of(task, events) {
            self.waiting.insert(id.clone(), waiting);
        }
        if let Some(label) = texts.label {
            self.labels.insert(id.clone(), label);
        }
        if let Some(activity) = activity {
            let per_terminal = activity
                .iter()
                .map(|(terminal, line)| (terminal.clone(), line.text.clone()))
                .collect();
            self.terminal_activity.insert(id.clone(), per_terminal);
        }
    }
}

/// What a task row says outside its line.
pub(super) struct RowTexts {
    /// The title's tooltip, masked for a blurred project.
    pub(super) tip: String,
    /// The short reason of an attention row.
    pub(super) label: Option<String>,
}

pub(super) fn row_texts(
    task: &Task,
    project: Option<&str>,
    events: Option<&[StoredEvent]>,
    activity: Option<&str>,
    subtasks: Option<&str>,
    blurred: bool,
) -> RowTexts {
    let comet = comet_of(task);
    let step = step_text(comet.at, comet.of);
    let reason = card_line(task, events).filter(|_| row_tier(task.state) == RowTier::Attention);
    let pr_summary = pull_request_summary(task.pull_request_status().as_ref());
    let agent = task.agent.as_deref().map(|id| agent_label(Some(id)));
    let tip = RowTip {
        project,
        id: &task.id,
        step: step.as_deref(),
        stage: comet.stage.as_deref(),
        subtasks,
        activity,
        pr: task.pr.as_deref(),
        pr_summary: pr_summary.as_deref(),
        agent: agent.as_deref(),
        reason: reason.as_deref(),
    };
    RowTexts {
        tip: row_tooltip(&tip, blurred),
        label: short_reason(task.state, comet.stage.as_deref(), blurred),
    }
}
