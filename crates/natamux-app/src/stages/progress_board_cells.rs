//! The parts of a section of the Progress layout: the cells of a row in its three parts, the
//! header of a stage column and the done disclosure. Split from `progress_board.rs`, which holds
//! the entity and the section.

use gpui::{ClickEvent, FontWeight, Hsla};
use natamux_proto::agents::activity::latest_line;
use natamux_proto::stages::progress::{StageProgress, stage_progress};
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{TintColor, Tooltip, prelude::*};

use super::{NEXT_WIDTH, ProgressBoard, ROW_HEIGHT, STAGE_WIDTH, TASK_WIDTH};
use crate::components::chips::{CountBadge, StateChip};
use crate::components::colors::{signal, state_colors};
use crate::stages::progress_board_rules::{
    Section, StageColumn, cell_links, next_button, next_text,
};
use crate::stages::stage_marker::marker;
use crate::stages::stage_view_rules::{LinkKind, format_duration};
use crate::tasks::task_rules::lane_summary;

pub(super) fn head_label(text: &'static str) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .color(Color::Muted)
        .weight(FontWeight::MEDIUM)
}

/// A horizontal line between two markers.
pub(super) fn link(kind: LinkKind, cx: &App) -> impl IntoElement + use<> {
    let line = div()
        .flex_1()
        .min_w(rems_from_px(8.0_f32))
        .h(rems_from_px(0.0_f32));
    match kind {
        LinkKind::Hidden => line,
        LinkKind::Done | LinkKind::Flow => line
            .border_t_2()
            .border_color(state_colors(TaskState::Done, cx).fg),
        LinkKind::Upcoming => line
            .border_t_2()
            .border_dashed()
            .border_color(cx.theme().colors().border),
    }
}

impl ProgressBoard {
    /// The tint of a row, on the three parts alike.
    pub(super) fn row_bg(&self, id: &str, cx: &App) -> Hsla {
        if self.hovered.as_deref() == Some(id) {
            cx.theme().colors().ghost_element_hover
        } else {
            gpui::transparent_black()
        }
    }

    pub(super) fn row_part(
        &self,
        part: &'static str,
        id: &str,
        cx: &Context<Self>,
    ) -> gpui::Stateful<Div> {
        let hover_id = id.to_owned();
        h_flex()
            .id(SharedString::from(format!("progress-{part}-{id}")))
            .h(px(ROW_HEIGHT))
            .border_b_1()
            .border_color(cx.theme().colors().border_variant)
            .bg(self.row_bg(id, cx))
            .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                this.set_hover(&hover_id, *hovered, cx)
            }))
    }

    pub(super) fn task_cell(&self, task: &Task, now: i64, cx: &Context<Self>) -> impl IntoElement {
        let id = task.id.clone();
        let open_id = task.id.clone();
        let title = div()
            .id(SharedString::from(format!("progress-open-{id}")))
            .min_w_0()
            .overflow_hidden()
            .cursor_pointer()
            .child(
                Label::new(task.title.clone())
                    .weight(FontWeight::MEDIUM)
                    .truncate(),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_task(&open_id, cx)));
        let chip = div()
            .id(SharedString::from(format!("progress-state-{id}")))
            .flex_none()
            .child(StateChip::new(task.state))
            .when_some(lane_summary(task), |chip, tip| {
                chip.tooltip(Tooltip::text(tip))
            });
        let time = task
            .stage_since
            .as_deref()
            .filter(|_| task.state != TaskState::Done)
            .map(|since| format_duration(since, now));
        let meta = h_flex()
            .gap_2()
            .child(
                Label::new(id.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .buffer_font(cx),
            )
            .child(chip)
            .children(
                time.map(|time| Label::new(time).size(LabelSize::XSmall).color(Color::Muted)),
            );
        self.row_part("task", &id, cx)
            .flex_none()
            .w(px(TASK_WIDTH))
            .px_4()
            .flex_col()
            .items_start()
            .justify_center()
            .gap_0p5()
            .child(title)
            .child(meta)
    }

    pub(super) fn next_cell(&self, task: &Task, now: i64, cx: &Context<Self>) -> impl IntoElement {
        let id = task.id.clone();
        let open_id = task.id.clone();
        let body = match next_button(task, now) {
            Some(label) => Button::new(SharedString::from(format!("progress-next-{id}")), label)
                .style(ButtonStyle::Tinted(TintColor::Accent))
                .size(ButtonSize::Compact)
                .on_click(cx.listener(move |this, _, _, cx| this.open_task(&open_id, cx)))
                .into_any_element(),
            None => {
                let activity = self
                    .dashboard
                    .read(cx)
                    .data
                    .activity
                    .get(&task.id)
                    .and_then(|lines| latest_line(lines));
                Label::new(next_text(task, activity))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .truncate()
                    .into_any_element()
            }
        };
        self.row_part("next", &id, cx)
            .flex_none()
            .w(px(NEXT_WIDTH))
            .px_4()
            .justify_end()
            .child(body)
    }

    pub(super) fn stage_cells(
        &self,
        task: &Task,
        steps: &[Step],
        cx: &Context<Self>,
    ) -> impl IntoElement {
        let progress = stage_progress(task, &[], steps);
        let links = cell_links(&progress);
        let raised = cx.theme().colors().element_background;
        let cells = progress.iter().zip(links).map(|(entry, (left, right))| {
            let key = format!("{}-{}", task.id, entry.stage);
            h_flex()
                .flex_1()
                .min_w(px(STAGE_WIDTH))
                .h_full()
                .when(entry.group.is_some() || entry.path.is_some(), |cell| {
                    cell.bg(raised)
                })
                .child(link(left, cx))
                .child(marker_of(&key, entry, cx))
                .child(link(right, cx))
        });
        self.row_part("stages", &task.id, cx)
            .w_full()
            .min_w(px(progress.len() as f32 * STAGE_WIDTH))
            .children(cells)
    }
}

fn marker_of(key: &str, entry: &StageProgress, cx: &App) -> AnyElement {
    marker(key, entry.status, entry.state, false, cx)
}

impl ProgressBoard {
    pub(super) fn column_head(&self, column: &StageColumn, cx: &App) -> impl IntoElement {
        let raised = cx.theme().colors().element_background;
        v_flex()
            .id(SharedString::from(format!(
                "progress-column-{}",
                column.stage_id
            )))
            .flex_1()
            .min_w(px(STAGE_WIDTH))
            .h_full()
            .px_1()
            .items_center()
            .justify_center()
            .overflow_hidden()
            .when(column.in_group || column.in_path, |head| head.bg(raised))
            .child(
                Label::new(column.name.clone())
                    .size(LabelSize::XSmall)
                    .weight(FontWeight::MEDIUM),
            )
            .child(h_flex().gap_1().child(CountBadge::new(column.here)).when(
                column.waiting > 0,
                |counts| {
                    counts.child(
                        Label::new(format!("{} wait", column.waiting))
                            .size(LabelSize::XSmall)
                            .weight(FontWeight::MEDIUM)
                            .color(Color::Custom(signal(cx).fg)),
                    )
                },
            ))
    }

    pub(super) fn disclosure(&self, section: &Section, cx: &Context<Self>) -> impl IntoElement {
        let key = section.key.clone();
        let open = self.expanded.contains(&section.key);
        let count = section.done.len();
        let hover_bg = cx.theme().colors().ghost_element_hover;
        h_flex()
            .id(SharedString::from(format!("progress-done-{}", section.key)))
            .h(rems_from_px(36.0_f32))
            .px_4()
            .gap_1p5()
            .cursor_pointer()
            .hover(move |row| row.bg(hover_bg))
            .child(
                Icon::new(if open {
                    IconName::ChevronDown
                } else {
                    IconName::ChevronRight
                })
                .size(IconSize::XSmall)
                .color(Color::Muted),
            )
            .child(
                Label::new(format!(
                    "{count} done {}",
                    if count == 1 { "task" } else { "tasks" }
                ))
                .size(LabelSize::XSmall)
                .weight(FontWeight::MEDIUM)
                .color(Color::Muted),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_done(&key, cx)))
    }
}
