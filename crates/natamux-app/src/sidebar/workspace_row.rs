//! The rows of the sidebar tree (docs/ui-spec.md 4.9, `WorkspaceRow.tsx`): a one-line task row
//! (state glyph, title, right slot), a terminal row under the open task, and the inline rename
//! field. Everything else a row used to show is in the title's tooltip. The tree draws the rows
//! as a flat list, so each row can be scrolled to.

use gpui::{AnyElement, ClickEvent, FontWeight, MouseButton, MouseDownEvent, Stateful};
use natamux_proto::agents::activity::latest_line;
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::blur_bar::{BlurBar, private_text};
use super::sidebar::{agent_mark, lucide, mono, small};
use super::workspace_tree::{OFFSET_MINUTES, Snapshot, WorkspaceTree};
use crate::agents::agent_choices::agent_label;
use crate::app::dashboard_data::DashboardData;
use crate::app::view_rules::is_unseen;
use crate::components::colors::state_colors;
use crate::components::glyph::StateGlyph;
use crate::components::shape::ShapeSize;
use crate::sidebar::comet::comet_of;
use crate::sidebar::shell_rows::{ShellRow, shell_row_name, shell_tag, task_glyph_state};
use crate::sidebar::sidebar_text::{
    RowTip, card_line, pull_request_summary, row_tooltip, short_reason, step_text,
};
use crate::sidebar::sidebar_tree::{RowTier, row_tier, subtask_count};
use crate::sidebar::tree_model::TreeModel;
use crate::sidebar::workspace_groups::{DEFAULT_REPO_ID, is_quiet};
use crate::tasks::task_rules::lane_summary;
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

    pub(super) fn task_row(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let key = format!("task:{}", task.id);
        let current = snap.current.as_deref() == Some(task.id.as_str());
        let repo = task.repo.as_deref().unwrap_or(DEFAULT_REPO_ID);
        let blurred = snap.blurred.iter().any(|id| id == repo);
        let renaming = self
            .rename
            .as_ref()
            .filter(|rename| rename.task_id == task.id);
        let is_renaming = renaming.is_some();
        let title_line = self.title_line(task, model, snap, is_renaming, blurred, cx);
        let title: AnyElement = match renaming {
            Some(rename) => self.rename_field(rename.editor.clone(), cx),
            None => title_line,
        };
        let colors = state_colors(task.state, cx);
        let group_name = SharedString::from(format!("row-{}", task.id));
        let open_id = task.id.clone();
        let menu_id = task.id.clone();
        // A bar on the left, not a tint, marks the rows that need the user.
        let bar = matches!(task.state, TaskState::Failed | TaskState::NeedsInput).then(|| {
            div()
                .absolute()
                .left_0()
                .top_0()
                .bottom_0()
                .w(px(2.))
                .bg(colors.fg)
        });
        let row = self
            .row_frame(&key, current, snap, cx)
            .id(SharedString::from(key.clone()))
            .group(group_name.clone())
            .relative()
            .flex()
            .items_center()
            .h(rems_from_px(26.0_f32))
            .px(rems_from_px(10.0_f32))
            .child(title)
            .children(bar)
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(format!("task:{open_id}"), window, cx);
                tree.activate(&format!("task:{open_id}"), cx);
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |tree, event: &MouseDownEvent, window, cx| {
                    tree.open_row_menu(&menu_id, event.position, window, cx);
                    cx.stop_propagation();
                }),
            );
        let menu_button = (!is_renaming).then(|| {
            let id = task.id.clone();
            div().absolute().top(px(1.)).right(px(4.)).child(
                IconButton::new(
                    SharedString::from(format!("menu-{}", task.id)),
                    IconName::Ellipsis,
                )
                .icon_size(IconSize::Small)
                .visible_on_hover(group_name.clone())
                .tooltip(Tooltip::text(format!("Actions for {}", task.id)))
                .on_click(cx.listener(
                    move |tree, event: &ClickEvent, window, cx| {
                        tree.open_row_menu(&id, event.position(), window, cx);
                        cx.stop_propagation();
                    },
                )),
            )
        });
        row.children(menu_button).into_any_element()
    }

    fn title_line(
        &self,
        task: &Task,
        model: &TreeModel,
        snap: &Snapshot,
        renaming: bool,
        blurred: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let unseen = is_unseen(&snap.seen, &task.id, task.state);
        let glyph = StateGlyph::new(
            SharedString::from(format!("glyph-{}", task.id)),
            Some(task_glyph_state(task, model.rows_of(&task.id))),
        )
        .quiet(is_quiet(task, snap.now_ms));
        let glyph = match lane_summary(task) {
            Some(tip) => glyph.tip(tip),
            None => glyph,
        };
        let rename_id = task.id.clone();
        let tip = snap.tips.get(&task.id).cloned().unwrap_or_default();
        h_flex()
            .w_full()
            .h(rems_from_px(16.0_f32))
            .gap_2()
            .child(glyph)
            .child(
                div()
                    .id(SharedString::from(format!("title-{}", task.id)))
                    .min_w_0()
                    .flex_1()
                    .child(title_label(task, unseen, blurred))
                    .tooltip(Tooltip::text(tip))
                    // A double click on the title is a shortcut for F2.
                    .on_click(cx.listener(move |tree, event: &ClickEvent, window, cx| {
                        if event.click_count() == 2 {
                            tree.start_rename(&rename_id, window, cx);
                            cx.stop_propagation();
                        }
                    })),
            )
            .when(unseen, |line| {
                line.child(
                    div()
                        .flex_none()
                        .size(rems_from_px(6.0_f32))
                        .rounded_full()
                        .bg(cx.theme().colors().text_accent),
                )
            })
            .when(!renaming, |line| line.child(right_slot(task, snap, cx)))
            .into_any_element()
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
        let open = key.clone();
        self.row_frame(&key, false, snap, cx)
            .id(SharedString::from(key.clone()))
            .h(rems_from_px(24.0_f32))
            .px_2()
            .flex()
            .items_center()
            .gap_2()
            .child(glyph)
            .children(agent)
            .child(label)
            .child(div().min_w_0().flex_1().children(activity))
            .children(tag)
            .tooltip(Tooltip::text(shell_row_name(row)))
            .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                tree.focus_key(open.clone(), window, cx);
                tree.activate(&open, cx);
            }))
            .into_any_element()
    }

    /// The field that takes the title's place while renaming: Enter saves, Esc cancels (the tree
    /// handles `Back`), and leaving it saves.
    fn rename_field(
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

/// The title in the look of its tier: bold when unseen or waiting, muted when finished.
fn title_label(task: &Task, unseen: bool, blurred: bool) -> AnyElement {
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
    private_text(
        blurred,
        || BlurBar::new(task.title.clone(), size).weight(weight),
        plain,
    )
}

/// The age of the last change; the menu button takes its place on hover.
fn age_label(task: &Task, snap: &Snapshot) -> Stateful<Div> {
    let age = format_age(
        &task.updated_at,
        snap.now_ms,
        AgeStyle::Short,
        OFFSET_MINUTES,
    );
    div()
        .id(SharedString::from(format!("age-{}", task.id)))
        .flex_none()
        .group_hover(SharedString::from(format!("row-{}", task.id)), |style| {
            style.invisible()
        })
        .child(small(age))
        .tooltip(Tooltip::text(format_exact(
            &task.updated_at,
            OFFSET_MINUTES,
        )))
}

/// The right slot of a task row: the short reason in the state colour on an attention row, else
/// the age. The menu button takes its place on hover.
fn right_slot(task: &Task, snap: &Snapshot, cx: &App) -> AnyElement {
    let Some(label) = snap.labels.get(&task.id) else {
        return age_label(task, snap).into_any_element();
    };
    div()
        .flex_none()
        .group_hover(SharedString::from(format!("row-{}", task.id)), |style| {
            style.invisible()
        })
        .child(
            Label::new(label.clone())
                .size(LabelSize::Custom(rems_from_px(12_f32)))
                .color(Color::Custom(state_colors(task.state, cx).fg))
                .single_line(),
        )
        .into_any_element()
}

impl Snapshot {
    /// Fills what the row of `task` says outside its line: the tooltip, the short reason and what
    /// each terminal's agent does.
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
