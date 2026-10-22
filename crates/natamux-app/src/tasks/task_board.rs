//! The task board (`features/tasks/TaskBoard.tsx`, ui-spec 4.26): one column per state, the
//! states that need the user first. Empty columns fold until opened, done and cancelled show
//! only with Show finished, and their columns can delete all their tasks.

use gpui::{ClickEvent, Entity, FocusHandle, Focusable, FontWeight, KeyContext, ScrollHandle};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::table_rules::{Step, pull_request_number, step};
use super::task_confirms::confirm_delete_all;
use super::task_rules::{needs_me, stage_label, state_label};
use super::task_table_cells::{RowData, diff_label, quiet_chip};
use super::task_time::{AgeStyle, format_age, format_exact};
use super::task_view::now_ms;
use super::task_view_rules::{BoardFolds, board_states, task_count};
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::BulkRun;
use crate::app::shortcuts::{OpenSelected, SelectFirst, SelectLast, SelectNext, SelectPrevious};
use crate::components::agent_badge::agent_name;
use crate::components::chips::{CountBadge, StateChip};
use crate::components::colors::state_colors;
use crate::components::empty_state::EmptyState;

const COLUMN_WIDTH: f32 = 272.;

pub struct TaskBoard {
    dashboard: Entity<Dashboard>,
    /// In the order to show, filtered; the task view holds the order still.
    rows: Vec<Task>,
    folds: BoardFolds,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// A card drawn with the hover look whatever the pointer does: the `board-hover-card` shot.
    hover_shown: Option<String>,
}

impl Focusable for TaskBoard {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TaskBoard {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        TaskBoard {
            dashboard,
            rows: Vec::new(),
            folds: BoardFolds::default(),
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            hover_shown: None,
        }
    }

    /// The first card that the board shows, for the hover shot.
    pub fn first_card(&self, cx: &App) -> Option<String> {
        self.shown(cx).first().map(|task| task.id.clone())
    }

    pub fn show_hover(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.hover_shown = id;
        cx.notify();
    }

    pub fn set_rows(&mut self, rows: Vec<Task>, cx: &mut Context<Self>) {
        if self.rows != rows {
            self.rows = rows;
            cx.notify();
        }
    }

    fn fold(&mut self, state: TaskState, fold: bool, cx: &mut Context<Self>) {
        self.folds.set(state, fold);
        cx.notify();
    }

    /// The cards in reading order: column by column, top to bottom.
    fn shown(&self, cx: &App) -> Vec<Task> {
        let dashboard = self.dashboard.read(cx);
        let states = board_states(dashboard.state_filter, dashboard.view().show_finished);
        states
            .iter()
            .flat_map(|state| self.rows.iter().filter(move |task| task.state == *state))
            .cloned()
            .collect()
    }

    fn move_by(&mut self, to: Step, cx: &mut Context<Self>) {
        let shown = self.shown(cx);
        let selected = self.dashboard.read(cx).selected.clone();
        let current = shown
            .iter()
            .position(|task| Some(&task.id) == selected.as_ref());
        if let Some(task) = step(shown.len(), current, to).and_then(|index| shown.get(index)) {
            let id = task.id.clone();
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.select(Some(id), cx));
        }
    }

    fn open_selected(&mut self, _: &OpenSelected, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.dashboard.read(cx).selected.clone() else {
            return;
        };
        if self.rows.iter().any(|task| task.id == id) {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.open(&id, cx));
        }
    }

    fn click(&mut self, id: &str, event: &ClickEvent, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        let id = id.to_string();
        self.dashboard.update(cx, |dashboard, cx| {
            if event.click_count() > 1 {
                dashboard.open(&id, cx);
            } else {
                dashboard.choose(&id, cx);
            }
        });
    }

    fn render_card(
        &self,
        index: usize,
        row: &RowData,
        selected: bool,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let task = &row.task;
        let id = task.id.clone();
        let urgent = needs_me(task.state);
        // The hover border is the web's line-strong (`border`).
        let border = if selected {
            colors.border_focused
        } else if self.hover_shown.as_ref() == Some(&task.id) {
            colors.border
        } else {
            colors.border_variant
        };
        let meta = card_meta(row);
        v_flex()
            .id(("board-card", index))
            .flex_none()
            .gap(px(6.))
            .px(px(12.))
            .py(px(10.))
            .rounded(px(8.))
            .bg(colors.surface_background)
            .border_1()
            .border_color(border)
            .when(!selected, |card| {
                card.hover(|card| card.border_color(colors.border))
            })
            // A card that needs the user is tinted on its left edge: look here first.
            .when(urgent, |card| {
                card.relative().child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .rounded_l(px(8.))
                        .bg(state_colors(task.state, cx).fg),
                )
            })
            .cursor_pointer()
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.click(&id, event, window, cx)
            }))
            .child(card_top(row, cx))
            .child(
                Label::new(task.title.clone())
                    .weight(FontWeight::MEDIUM)
                    .line_clamp(2),
            )
            .when(!meta.is_empty(), |card| {
                card.child(h_flex().flex_wrap().gap_x_3().children(meta))
            })
    }

    fn render_folded(
        &self,
        index: usize,
        state: TaskState,
        count: usize,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let label = state_label(state);
        // gpui has no vertical text, so the folded column names its state in the tooltip.
        v_flex()
            .id(("board-folded", index))
            .flex_none()
            .w(px(40.))
            .items_center()
            .gap_2()
            .py_3()
            .rounded(px(8.))
            .bg(colors.surface_background.opacity(0.6))
            .cursor_pointer()
            .hover(|column| column.bg(colors.ghost_element_hover))
            .tooltip(Tooltip::text(format!(
                "Expand {label}, {}",
                task_count(count)
            )))
            .on_click(cx.listener(move |this, _, _, cx| this.fold(state, false, cx)))
            .child(
                div()
                    .size(px(6.))
                    .rounded_full()
                    .bg(state_colors(state, cx).fg),
            )
            .child(CountBadge::new(count))
    }

    fn render_column(
        &self,
        index: usize,
        state: TaskState,
        cards: Vec<AnyElement>,
        tasks: Vec<Task>,
        cx: &Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let label = state_label(state);
        let deletable = state.is_finished() && !tasks.is_empty();
        let deleting = self.dashboard.read(cx).bulk.running(BulkRun::DeleteTasks);
        let dashboard = self.dashboard.clone();
        let empty = cards.is_empty();
        v_flex()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .min_h_0()
            .p_2()
            .rounded(px(8.))
            .bg(colors.surface_background.opacity(0.6))
            .child(
                h_flex()
                    .flex_none()
                    .h(px(36.))
                    .px_1()
                    .gap_2()
                    .child(StateChip::new(state))
                    .child(CountBadge::new(cards.len()))
                    .child(div().flex_1())
                    .when(deletable, |header| {
                        header.child(
                            IconButton::new(("board-delete", index), IconName::Trash)
                                .icon_size(IconSize::Small)
                                .disabled(deleting)
                                .tooltip(Tooltip::text(format!(
                                    "Delete all {} tasks",
                                    label.to_lowercase()
                                )))
                                .on_click(move |_, window, cx| {
                                    confirm_delete_all(&dashboard, state, &tasks, window, cx)
                                }),
                        )
                    })
                    .child(
                        IconButton::new(("board-collapse", index), IconName::ChevronDown)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text(format!("Collapse {label}")))
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.fold(state, true, cx)),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .id(("board-cards", index))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .gap_2()
                    .p(px(2.))
                    .when(empty, |list| {
                        list.child(
                            Label::new("Nothing here")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    })
                    .children(cards),
            )
    }
}

/// Line 1 of a card: id, the quiet chip, and the age on the right.
fn card_top(row: &RowData, cx: &App) -> impl IntoElement + use<> {
    let task = &row.task;
    let age = format_age(&task.updated_at, row.now, AgeStyle::Short, 0);
    h_flex()
        .gap_2()
        .child(
            Label::new(task.id.clone())
                .size(LabelSize::Small)
                .color(Color::Muted)
                .buffer_font(cx),
        )
        .when(row.quiet, |line| line.child(quiet_chip(cx)))
        .child(
            div()
                .id("age")
                .ml_auto()
                .child(Label::new(age).size(LabelSize::Small).color(Color::Muted))
                .tooltip(Tooltip::text(format_exact(&task.updated_at, 0))),
        )
}

/// Line 3 of a card: stage, agent, `+12 −3`, PR number and what it waits for.
fn card_meta(row: &RowData) -> Vec<AnyElement> {
    let task = &row.task;
    let text = |text: String| {
        Label::new(text)
            .size(LabelSize::Small)
            .color(Color::Muted)
            .into_any_element()
    };
    let meta = [
        stage_label(task).map(text),
        task.agent
            .as_deref()
            .map(|agent| agent_name(agent, LabelSize::Small).into_any_element()),
        diff_label(row.stat.as_ref()),
        task.pr.as_deref().map(|url| text(pull_request_number(url))),
        (!row.waits.is_empty()).then(|| text(format!("waits for {}", row.waits))),
    ];
    meta.into_iter().flatten().collect()
}

impl Render for TaskBoard {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = now_ms();
        let (states, selected, rows) = {
            let dashboard = self.dashboard.read(cx);
            let states = board_states(dashboard.state_filter, dashboard.view().show_finished);
            let rows: Vec<RowData> = self
                .rows
                .iter()
                .map(|task| RowData::of(task, &dashboard.data, now))
                .collect();
            (states, dashboard.selected.clone(), rows)
        };
        let columns_count = states.len();
        let mut columns: Vec<AnyElement> = Vec::new();
        for (index, state) in states.into_iter().enumerate() {
            let in_column: Vec<(usize, &RowData)> = rows
                .iter()
                .enumerate()
                .filter(|(_, row)| row.task.state == state)
                .collect();
            if self.folds.is_folded(state, in_column.len(), columns_count) {
                columns.push(
                    self.render_folded(index, state, in_column.len(), cx)
                        .into_any_element(),
                );
                continue;
            }
            let tasks: Vec<Task> = in_column.iter().map(|(_, row)| row.task.clone()).collect();
            let cards: Vec<AnyElement> = in_column
                .iter()
                .map(|(row_index, row)| {
                    let picked = selected.as_deref() == Some(row.task.id.as_str());
                    self.render_card(*row_index, row, picked, cx)
                        .into_any_element()
                })
                .collect();
            columns.push(
                self.render_column(index, state, cards, tasks, cx)
                    .into_any_element(),
            );
        }
        let mut context = KeyContext::default();
        context.add("NatamuxList");
        context.add("not_editing");
        h_flex()
            .id("task-board-columns")
            .key_context(context)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_by(Step::Next, cx)))
            .on_action(
                cx.listener(|this, _: &SelectPrevious, _, cx| this.move_by(Step::Previous, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.move_by(Step::First, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.move_by(Step::Last, cx)))
            .on_action(cx.listener(Self::open_selected))
            .size_full()
            .items_start()
            .gap_3()
            .px_4()
            .pb_4()
            .overflow_x_scroll()
            .track_scroll(&self.scroll)
            .when(columns_count == 0, |board| {
                board.child(
                    EmptyState::new("board-no-columns", "list-todo", "No columns to show")
                        .body("Pick a state, or turn on Show finished."),
                )
            })
            .children(columns)
    }
}
