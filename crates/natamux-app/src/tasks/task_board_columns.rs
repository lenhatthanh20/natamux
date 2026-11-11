//! The columns of the task board. Each column draws its cards through a `list`: only the cards
//! on screen are built, so hundreds of tasks cost a few cards a frame.

use gpui::{ListAlignment, ListState, list};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{Tooltip, prelude::*};

use super::cap::more_label;
use super::{COLUMN_WIDTH, TaskBoard};
use crate::components::button_style::quiet_button;
use crate::components::chips::{CountBadge, StateChip};
use crate::tasks::task_confirms::confirm_delete_all;
use crate::tasks::task_rules::state_label;
use crate::tasks::task_table_cells::RowData;
use crate::tasks::task_view::now_ms;

/// Pixels the list builds above and below the window, so a scroll shows no blank.
const OVERDRAW: f32 = 400.;

/// What one column draws now, set again on every draw of the board.
pub struct Column {
    /// Indexes into `rows` of the cards drawn, in the column's order.
    cards: Vec<usize>,
    /// Cards the cap leaves out: "Show N more" is the last list item.
    hidden: usize,
    list: ListState,
}

impl Column {
    fn new() -> Self {
        Column {
            cards: Vec::new(),
            hidden: 0,
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
        }
    }

    fn items(&self) -> usize {
        self.cards.len() + usize::from(self.hidden > 0)
    }
}

impl TaskBoard {
    #[cfg(test)]
    pub fn list_for(&self, state: TaskState) -> Option<ListState> {
        let (_, column) = self.columns.iter().find(|(each, _)| *each == state)?;
        Some(column.list.clone())
    }

    /// Takes the cards of a column; the list keeps its scroll top when the count changes.
    fn sync(&mut self, state: TaskState, cards: Vec<usize>, hidden: usize) {
        let at = match self.columns.iter().position(|(each, _)| *each == state) {
            Some(at) => at,
            None => {
                self.columns.push((state, Column::new()));
                self.columns.len() - 1
            }
        };
        let column = &mut self.columns[at].1;
        column.cards = cards;
        column.hidden = hidden;
        let items = column.items();
        if column.list.item_count() != items {
            let top = column.list.logical_scroll_top();
            column.list.reset(items);
            column.list.scroll_to(top);
        }
        // New data can change a card's height, not only the count.
        column.list.remeasure_items(0..items);
    }

    /// Scrolls the column that holds the task so its card shows.
    pub(super) fn reveal(&self, id: &str) {
        for (_, column) in &self.columns {
            let found = column
                .cards
                .iter()
                .position(|at| self.rows.get(*at).is_some_and(|task| task.id == id));
            if let Some(index) = found {
                column.list.scroll_to_reveal_item(index);
            }
        }
    }

    /// Item `index` of a column's list, built when the list shows it.
    fn column_item(&self, state: TaskState, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some((_, column)) = self.columns.iter().find(|(each, _)| *each == state) else {
            return div().into_any_element();
        };
        if column.hidden > 0 && index == column.cards.len() {
            let button = quiet_button(("board-more", index), more_label(column.hidden))
                .label_size(LabelSize::Small)
                .on_click(cx.listener(move |this, _, _, cx| this.more(state, cx)));
            return div().w_full().child(button).into_any_element();
        }
        let Some((at, task)) = column
            .cards
            .get(index)
            .and_then(|at| self.rows.get(*at).map(|task| (*at, task)))
        else {
            return div().into_any_element();
        };
        #[cfg(test)]
        self.built.set(self.built.get() + 1);
        let dashboard = self.dashboard.read(cx);
        let picked = dashboard.selected.as_deref() == Some(task.id.as_str());
        let row = RowData::of(task, &dashboard.data, now_ms());
        let card = self.render_card(at, &row, picked, cx);
        // A list has no gap: the old gap_2 is padding under each card but the last.
        let last = index + 1 == column.items();
        div()
            .w_full()
            .when(!last, |item| item.pb_2())
            .child(card)
            .into_any_element()
    }

    fn render_column(
        &self,
        index: usize,
        state: TaskState,
        total: usize,
        list_state: ListState,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let items =
            cx.processor(move |board, index: usize, _, cx| board.column_item(state, index, cx));
        let colors = cx.theme().colors();
        let label = state_label(state);
        let deletable = state.is_finished() && total > 0;
        let deleting = self.dashboard.read(cx).deletes.batch_queued();
        v_flex()
            .flex_none()
            .w(px(COLUMN_WIDTH))
            .h_full()
            .min_h_0()
            .p_2()
            .rounded_md()
            .bg(colors.surface_background.opacity(0.6))
            .child(
                h_flex()
                    .flex_none()
                    .h(rems_from_px(36.0_f32))
                    .px_1()
                    .gap_2()
                    .child(StateChip::new(state))
                    .child(CountBadge::new(total))
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
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    // Collected on click, not on every draw.
                                    let tasks: Vec<Task> = this
                                        .rows
                                        .iter()
                                        .filter(|task| task.state == state)
                                        .cloned()
                                        .collect();
                                    confirm_delete_all(&this.dashboard, state, &tasks, window, cx)
                                })),
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
                    .p(rems_from_px(2.0_f32))
                    .map(|body| {
                        if total == 0 {
                            body.child(
                                Label::new("Nothing here")
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                        } else {
                            body.child(list(list_state, items).size_full())
                        }
                    }),
            )
    }

    /// One state's column, folded or with its cards.
    pub(super) fn render_state(
        &mut self,
        index: usize,
        state: TaskState,
        columns_count: usize,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let in_column: Vec<usize> = (0..self.rows.len())
            .filter(|at| self.rows[*at].state == state)
            .collect();
        let total = in_column.len();
        if self.folds.is_folded(state, total, columns_count) {
            let folded = self.render_folded(index, state, total, cx);
            return folded.into_any_element();
        }
        let cards: Vec<usize> = {
            let tasks: Vec<&Task> = in_column.iter().map(|at| &self.rows[*at]).collect();
            let drawn = self.limits.drawn(state, &tasks);
            in_column
                .iter()
                .zip(drawn)
                .filter(|(_, drawn)| *drawn)
                .map(|(at, _)| *at)
                .collect()
        };
        let hidden = total - cards.len();
        self.sync(state, cards, hidden);
        let list_state = self
            .columns
            .iter()
            .find(|(each, _)| *each == state)
            .map(|(_, column)| column.list.clone())
            .unwrap_or_else(|| ListState::new(0, ListAlignment::Top, px(OVERDRAW)));
        self.render_column(index, state, total, list_state, cx)
            .into_any_element()
    }
}
