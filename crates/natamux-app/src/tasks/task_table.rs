//! The task table (`features/tasks/TaskTable.tsx`, ui-spec 4.25): a virtual list of 28 px rows,
//! columns by the main width (`table_columns`), sortable headers, a card list under 560, one tab
//! stop with j/k, arrows, Home/End, Enter, Shift+F10 and type-ahead, and the row menu.

use std::time::Instant;

use gpui::{
    ClickEvent, Entity, EventEmitter, FocusHandle, Focusable, FontWeight, KeyContext, KeyDownEvent,
    MouseButton, MouseDownEvent, Point, ScrollStrategy, Stateful, UniformListScrollHandle, point,
    uniform_list,
};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::table_columns::{ColumnKey, HasValue, TableLayout, column_label, table_layout};
use super::table_menu::build_row_menu;
use super::table_rules::{Step, TYPE_AHEAD_MS, step, type_ahead_target};
use super::task_rules::{lane_summary, stage_state_label};
use super::task_sort::SortDirection;
use super::task_table_cells::{RowData, cell, header_align_end};
use super::task_time::{AgeStyle, format_age};
use super::task_view::now_ms;
use crate::app::dashboard::Dashboard;
use crate::app::shortcuts::SelectPrevious;
use crate::app::shortcuts::{OpenRowMenu, OpenSelected, SelectFirst, SelectLast, SelectNext};
use crate::components::glyph::StateGlyph;
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

const ROW: f32 = 28.;
const CARD_ROW: f32 = 56.;
const HEADER: f32 = 26.;

/// A key moved the selection: the rows hold still for a moment (`onKeyboardMove`).
pub struct KeyMoved;

pub struct TaskTable {
    dashboard: Entity<Dashboard>,
    /// In the order to show: sorted, filtered and held still by the task view.
    rows: Vec<Task>,
    main_width: Option<f32>,
    focus: FocusHandle,
    scroll: UniformListScrollHandle,
    typed: String,
    typed_at: Option<Instant>,
}

impl EventEmitter<KeyMoved> for TaskTable {}

impl Focusable for TaskTable {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl TaskTable {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        TaskTable {
            dashboard,
            rows: Vec::new(),
            main_width: None,
            focus: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            typed: String::new(),
            typed_at: None,
        }
    }

    pub fn set_rows(&mut self, rows: Vec<Task>, main_width: Option<f32>, cx: &mut Context<Self>) {
        if self.rows != rows || self.main_width != main_width {
            self.rows = rows;
            self.main_width = main_width;
            cx.notify();
        }
    }

    /// For `--gate2`: scroll to a row.
    pub fn scroll_to(&self, index: usize) {
        self.scroll.scroll_to_item(index, ScrollStrategy::Top);
    }

    fn selected_index(&self, cx: &App) -> Option<usize> {
        let selected = self.dashboard.read(cx).selected.as_deref()?;
        self.rows.iter().position(|task| task.id == selected)
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(task) = self.rows.get(index) else {
            return;
        };
        let id = task.id.clone();
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.select(Some(id), cx));
        self.scroll.scroll_to_item(index, ScrollStrategy::Nearest);
        cx.emit(KeyMoved);
    }

    fn move_by(&mut self, to: Step, cx: &mut Context<Self>) {
        let current = self.selected_index(cx);
        if let Some(index) = step(self.rows.len(), current, to) {
            self.select_index(index, cx);
        }
    }

    fn open_selected(&mut self, _: &OpenSelected, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self
            .selected_index(cx)
            .and_then(|index| self.rows.get(index))
        {
            let id = task.id.clone();
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.open(&id, cx));
        }
    }

    /// Shift+F10 and the Menu key: the menu under the selected row.
    fn open_selected_menu(&mut self, _: &OpenRowMenu, window: &mut Window, cx: &mut Context<Self>) {
        let Some(index) = self.selected_index(cx) else {
            return;
        };
        let (bounds, offset) = {
            let state = self.scroll.0.borrow();
            (state.base_handle.bounds(), state.base_handle.offset())
        };
        let height = px(if self.is_cards() { CARD_ROW } else { ROW });
        let y = bounds.origin.y + offset.y + height * (index + 1) as f32;
        let x = bounds.origin.x + offset.x + px(self.layout(cx).task_column_x());
        self.open_menu(index, point(x, y), window, cx);
    }

    fn open_menu(
        &mut self,
        index: usize,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.rows.get(index).cloned() else {
            return;
        };
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.select(Some(task.id.clone()), cx)
        });
        window.focus(&self.focus, cx);
        let tasks = self.dashboard.read(cx).data.tasks.clone();
        let menu = build_row_menu(
            &task,
            &tasks,
            &self.dashboard,
            self.focus.clone(),
            window,
            cx,
        );
        MenuLayer::open(menu, position, window, cx);
    }

    fn click(
        &mut self,
        index: usize,
        event: &ClickEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        window.focus(&self.focus, cx);
        let Some(id) = self.rows.get(index).map(|task| task.id.clone()) else {
            return;
        };
        self.dashboard.update(cx, |dashboard, cx| {
            if event.click_count() > 1 {
                dashboard.open(&id, cx);
            } else {
                dashboard.choose(&id, cx);
            }
        });
    }

    /// Letters jump to a row by its title (`typeAheadTarget`). j, k and the action keys are
    /// bindings, so they never get here.
    fn type_ahead(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let key = &event.keystroke.key;
        if key.chars().count() != 1 || key == " " || event.keystroke.modifiers.modified() {
            return;
        }
        let fresh = self
            .typed_at
            .is_some_and(|at| at.elapsed().as_millis() <= TYPE_AHEAD_MS as u128);
        if !fresh {
            self.typed.clear();
        }
        self.typed.push_str(key);
        self.typed_at = Some(Instant::now());
        let titles: Vec<&str> = self.rows.iter().map(|task| task.title.as_str()).collect();
        let from = self.selected_index(cx).unwrap_or(0);
        if let Some(index) = type_ahead_target(&titles, &self.typed, from) {
            cx.stop_propagation();
            self.select_index(index, cx);
        }
    }

    fn is_cards(&self) -> bool {
        self.main_width.is_some_and(|width| width < 560.)
    }

    fn layout(&self, cx: &App) -> TableLayout {
        let data = &self.dashboard.read(cx).data;
        let has = HasValue {
            changes: self.rows.iter().any(|task| {
                data.diff_stats
                    .get(&task.id)
                    .is_some_and(|stat| stat.files > 0)
            }),
            pr: self.rows.iter().any(|task| task.pr.is_some()),
            cost: self
                .rows
                .iter()
                .any(|task| data.usage.contains_key(&task.id)),
        };
        table_layout(self.main_width, has)
    }

    fn render_header(&self, columns: &[(ColumnKey, Option<f32>)], cx: &Context<Self>) -> Div {
        let sort = self.dashboard.read(cx).sort;
        let colors = cx.theme().colors();
        h_flex()
            .flex_none()
            .h(px(HEADER))
            .border_b_1()
            .border_color(colors.border_variant)
            .children(columns.iter().map(|(column, width)| {
                let column = *column;
                let sorted = sort
                    .filter(|sort| sort.key == column)
                    .map(|sort| sort.direction);
                let arrow = match sorted {
                    Some(SortDirection::Desc) => Some("arrow-down"),
                    Some(SortDirection::Asc) => Some("arrow-up"),
                    None => None,
                };
                let dashboard = self.dashboard.clone();
                h_flex()
                    .id(("table-header", column as usize))
                    .h_full()
                    .px(rems_from_px(12.0_f32))
                    .gap_1()
                    .cursor_pointer()
                    .when_some(*width, |cell, width| cell.w(px(width)).flex_none())
                    .when(width.is_none(), |cell| {
                        cell.flex_1().min_w(rems_from_px(240.0_f32))
                    })
                    .when(header_align_end(column), |cell| cell.justify_end())
                    .on_click(move |_, _, cx| {
                        dashboard.update(cx, |dashboard, cx| dashboard.sort_by(column, cx))
                    })
                    .child(
                        Label::new(column_label(column))
                            .size(LabelSize::Small)
                            .color(if sorted.is_some() {
                                Color::Default
                            } else {
                                Color::Muted
                            }),
                    )
                    .when_some(arrow, |cell, arrow| {
                        cell.child(Icon::from_path(icon_path(arrow)).size(IconSize::XSmall))
                    })
            }))
    }

    fn row_frame(
        &self,
        index: usize,
        selected: bool,
        focused: bool,
        cx: &Context<Self>,
    ) -> Stateful<Div> {
        let colors = cx.theme().colors();
        let bg = match (selected, focused) {
            (true, true) => Some(colors.element_selection_background),
            (true, false) => Some(colors.ghost_element_selected),
            _ => None,
        };
        h_flex()
            .id(("task-row", index))
            .relative()
            .w_full()
            .border_b_1()
            .border_color(colors.border_variant)
            .cursor_pointer()
            .when_some(bg, |row, bg| row.bg(bg))
            .when(bg.is_none(), |row| {
                row.hover(|row| row.bg(colors.ghost_element_hover))
            })
            // The inset focus ring of the row with the list's focus (ui-spec 5.2), and the 2 px
            // bar on the left of the selected row.
            .when(selected && focused, |row| {
                row.child(
                    div()
                        .absolute()
                        .inset_0()
                        .border_1()
                        .border_color(colors.border_focused),
                )
            })
            .when(selected, |row| {
                row.child(
                    div()
                        .absolute()
                        .left_0()
                        .top_0()
                        .bottom_0()
                        .w(px(2.))
                        .bg(colors.border_focused),
                )
            })
            .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.click(index, event, window, cx)
            }))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    this.open_menu(index, event.position, window, cx)
                }),
            )
    }

    fn render_rows(
        &mut self,
        range: std::ops::Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let focused = self.focus.contains_focused(window, cx);
        let selected = self.selected_index(cx);
        let now = now_ms();
        let layout = self.layout(cx);
        let rows: Vec<(usize, RowData)> = {
            let dashboard = self.dashboard.read(cx);
            range
                .filter_map(|index| {
                    let task = self.rows.get(index)?;
                    Some((index, RowData::of(task, &dashboard.data, now)))
                })
                .collect()
        };
        rows.into_iter()
            .map(|(index, row)| {
                let frame = self.row_frame(index, selected == Some(index), focused, cx);
                match &layout {
                    TableLayout::Cards => frame
                        .h(px(CARD_ROW))
                        .child(card(&row, cx))
                        .into_any_element(),
                    TableLayout::Table { widths, .. } => frame
                        .h(px(ROW))
                        .children(widths.iter().map(|(column, width)| {
                            div()
                                .h_full()
                                .px(rems_from_px(12.0_f32))
                                .flex()
                                .items_center()
                                // An end-aligned number (Changes) may use the padding, not be cut.
                                .when(!header_align_end(*column), |cell| cell.overflow_hidden())
                                .when_some(*width, |cell, width| cell.w(px(width)).flex_none())
                                .when(width.is_none(), |cell| {
                                    cell.flex_1().min_w(rems_from_px(240.0_f32))
                                })
                                .when(header_align_end(*column), |cell| cell.justify_end())
                                .child(cell(*column, &row, cx))
                        }))
                        .into_any_element(),
                }
            })
            .collect()
    }
}

/// A row of the narrow list: glyph, title, and id · state · age below.
fn card(row: &RowData, cx: &App) -> impl IntoElement {
    let task = &row.task;
    let state = if row.quiet {
        "Quiet".to_string()
    } else {
        stage_state_label(task)
    };
    let glyph = StateGlyph::new("glyph", Some(task.state)).quiet(row.quiet);
    let glyph = match lane_summary(task) {
        Some(tip) => glyph.tip(tip),
        None => glyph,
    };
    h_flex()
        .w_full()
        .min_w_0()
        .gap_2()
        .px(rems_from_px(12.0_f32))
        .child(glyph)
        .child(
            v_flex()
                .min_w_0()
                .flex_1()
                .child(
                    Label::new(task.title.clone())
                        .weight(FontWeight::MEDIUM)
                        .truncate(),
                )
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(
                            Label::new(task.id.clone())
                                .size(LabelSize::Small)
                                .color(Color::Muted)
                                .buffer_font(cx),
                        )
                        .child(
                            Label::new("\u{b7}")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(Label::new(state).size(LabelSize::Small).color(Color::Muted))
                        .child(
                            Label::new("\u{b7}")
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                        .child(
                            Label::new(format_age(&task.updated_at, row.now, AgeStyle::Short, 0))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        ),
                ),
        )
}

impl Render for TaskTable {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let layout = self.layout(cx);
        let mut context = KeyContext::default();
        context.add("NatamuxList");
        context.add("not_editing");
        let list = uniform_list(
            "task-table-rows",
            self.rows.len(),
            cx.processor(|this, range, window, cx| this.render_rows(range, window, cx)),
        )
        .track_scroll(&self.scroll)
        .flex_1()
        .min_h_0();
        // The widths add up to at most the main width at each breakpoint, so the table never
        // needs to scroll sideways (ui-spec 4.25 overflow).
        let header = match &layout {
            TableLayout::Table { widths, .. } => Some(self.render_header(widths, cx)),
            TableLayout::Cards => None,
        };
        v_flex()
            .key_context(context)
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_by(Step::Next, cx)))
            .on_action(
                cx.listener(|this, _: &SelectPrevious, _, cx| this.move_by(Step::Previous, cx)),
            )
            .on_action(cx.listener(|this, _: &SelectFirst, _, cx| this.move_by(Step::First, cx)))
            .on_action(cx.listener(|this, _: &SelectLast, _, cx| this.move_by(Step::Last, cx)))
            .on_action(cx.listener(Self::open_selected))
            .on_action(cx.listener(Self::open_selected_menu))
            .on_key_down(cx.listener(Self::type_ahead))
            .size_full()
            .overflow_hidden()
            .children(header)
            .child(list)
    }
}
