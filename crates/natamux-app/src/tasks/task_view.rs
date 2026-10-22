//! All tasks in main (the main area of `Dashboard.tsx`, ui-spec 4.29): the view header with the
//! count, the state filter, Show finished and Table / Board; the quick add; then the table or the
//! board, or the skeleton, error or empty state. The rows keep one order while the pointer is over
//! them and 2 s after a key move (`frozen_order`).

use std::time::Duration;

use gpui::{Entity, EventEmitter, FocusHandle, Focusable, FontWeight, Subscription};
use natamux_proto::tasks::task::Task;
use ui::{ButtonLike, Checkbox, TintColor, Tooltip, prelude::*};

use super::add_subtask_dialog;
use super::frozen_order::{HOLD_MS, freeze_order, is_frozen};
use super::quick_add::{QuickAdd, StartTask};
use super::state_filter::{StateFilter, width_probe};
use super::task_attention::{count_by_state, filter_tasks, needs_me_tasks};
use super::task_board::TaskBoard;
use super::task_sort::sort_tasks;
use super::task_table::{KeyMoved, TaskTable};
use super::task_view_rules::{MainContent, NARROW_BELOW, filter_states, main_content};
use crate::app::commands::shortcut_of;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::Link;
use crate::app::shortcuts::{ShowBoard, ShowTable};
use crate::app::view_state::Layout;
use crate::components::chips::{CountBadge, Kbd};
use crate::components::empty_state::EmptyState;
use crate::components::segmented::Segmented;
use crate::components::skeleton::{Loading, SkeletonList};

pub use crate::app::clock::now_ms;

pub struct TaskView {
    dashboard: Entity<Dashboard>,
    quick_add: Entity<QuickAdd>,
    table: Entity<TaskTable>,
    board: Entity<TaskBoard>,
    layout_focus: FocusHandle,
    /// The width of the view header: the main width that picks columns and labels.
    main_width: Entity<Option<f32>>,
    pointer_inside: bool,
    held_at: Option<i64>,
    /// The order last shown, and what it was sorted and filtered by.
    shown_ids: Vec<String>,
    shown_key: String,
    _hold: Option<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<StartTask> for TaskView {}

impl Focusable for TaskView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.dashboard.read(cx).view().layout {
            Layout::Table => self.table.focus_handle(cx),
            Layout::Board => self.board.focus_handle(cx),
        }
    }
}

impl TaskView {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        add_subtask_dialog::init(&dashboard, cx);
        let quick_add = cx.new(|cx| QuickAdd::new(dashboard.clone(), window, cx));
        let table = cx.new(|cx| TaskTable::new(dashboard.clone(), cx));
        let board = cx.new(|cx| TaskBoard::new(dashboard.clone(), cx));
        let main_width = cx.new(|_| None);
        let subscriptions = vec![
            cx.observe(&dashboard, |this, _, cx| this.refresh(cx)),
            cx.observe(&main_width, |this, _, cx| this.refresh(cx)),
            cx.subscribe(&quick_add, |_, _, event: &StartTask, cx| {
                cx.emit(event.clone())
            }),
            cx.subscribe(&table, |this, _, _: &KeyMoved, cx| this.hold(cx)),
        ];
        let mut view = TaskView {
            dashboard,
            quick_add,
            table,
            board,
            layout_focus: cx.focus_handle().tab_stop(true),
            main_width,
            pointer_inside: false,
            held_at: None,
            shown_ids: Vec::new(),
            shown_key: String::new(),
            _hold: None,
            _subscriptions: subscriptions,
        };
        view.refresh(cx);
        view
    }

    /// `n`: the backlog field.
    pub fn focus_new_task(&self, window: &mut Window, cx: &mut App) {
        let field = self.quick_add.read(cx).focus_handle(cx);
        window.focus(&field, cx);
    }

    pub fn quick_add(&self) -> &Entity<QuickAdd> {
        &self.quick_add
    }

    /// The table, for `--gate2` scrolling.
    pub fn table(&self) -> &Entity<TaskTable> {
        &self.table
    }

    /// The board, for the hover shot.
    pub fn board(&self) -> &Entity<TaskBoard> {
        &self.board
    }

    /// Sorts, filters and holds the rows still, then hands them to the table and the board.
    fn refresh(&mut self, cx: &mut Context<Self>) {
        let (sorted, key) = {
            let dashboard = self.dashboard.read(cx);
            let data = &dashboard.data;
            let mut visible = filter_tasks(&data.tasks, dashboard.state_filter, &dashboard.filter);
            if dashboard.needs_you {
                visible = needs_me_tasks(&visible, now_ms());
            }
            let sorted = sort_tasks(&visible, dashboard.sort, &data.diff_stats, &data.usage);
            let key = format!(
                "{:?}|{:?}|{}|{}",
                dashboard.sort, dashboard.state_filter, dashboard.needs_you, dashboard.filter
            );
            (sorted, key)
        };
        let frozen = is_frozen(
            self.pointer_inside,
            self.held_at,
            now_ms(),
            key == self.shown_key,
        );
        let ordered: Vec<Task> = if frozen {
            freeze_order(&self.shown_ids, &sorted, |task| task.id.as_str())
        } else {
            sorted
        };
        self.shown_ids = ordered.iter().map(|task| task.id.clone()).collect();
        self.shown_key = key;
        let width = *self.main_width.read(cx);
        self.table
            .update(cx, |table, cx| table.set_rows(ordered.clone(), width, cx));
        self.board
            .update(cx, |board, cx| board.set_rows(ordered, cx));
        cx.notify();
    }

    /// A key moved the selection: the order holds for 2 s, then takes the live order again.
    fn hold(&mut self, cx: &mut Context<Self>) {
        self.held_at = Some(now_ms());
        self._hold = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(HOLD_MS as u64))
                .await;
            this.update(cx, |this, cx| this.refresh(cx)).ok();
        }));
    }

    fn set_pointer_inside(&mut self, inside: bool, cx: &mut Context<Self>) {
        if self.pointer_inside != inside {
            self.pointer_inside = inside;
            self.refresh(cx);
        }
    }

    fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.set_layout(layout, cx));
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let main_width = *self.main_width.read(cx);
        let narrow = main_width.is_some_and(|width| width < NARROW_BELOW);
        let dashboard = self.dashboard.read(cx);
        let layout = dashboard.view().layout;
        let show_finished = dashboard.view().show_finished;
        let searched = filter_tasks(&dashboard.data.tasks, None, &dashboard.filter);
        let states = filter_states(&count_by_state(&searched), dashboard.state_filter);
        let filter = StateFilter::new(
            self.dashboard.clone(),
            states,
            dashboard.state_filter,
            main_width,
        );
        let count = self.shown_ids.len();
        let toggle = self.dashboard.clone();
        let pick = self.dashboard.clone();
        let layouts = [
            ("Table".into(), IconName::Table),
            ("Board".into(), IconName::DiffSplit),
        ];
        let selected = usize::from(layout == Layout::Board);
        h_flex()
            .relative()
            .flex_none()
            .h(px(44.))
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(width_probe(&self.main_width))
            .child(
                Label::new("All tasks")
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(CountBadge::new(count))
            .children(dashboard.needs_you.then(|| self.needs_you_chip()))
            .child(filter)
            .child(
                h_flex()
                    .ml_auto()
                    .flex_none()
                    .gap_3()
                    .when(layout == Layout::Board, |row| {
                        row.child(
                            Checkbox::new("show-finished", show_finished.into())
                                .label(if narrow { "Finished" } else { "Show finished" })
                                .on_click(move |_, _, cx| {
                                    toggle.update(cx, |dashboard, cx| {
                                        dashboard.toggle_show_finished(cx)
                                    })
                                }),
                        )
                    })
                    .child(
                        Segmented::new(
                            "task-layout",
                            &self.layout_focus,
                            layouts,
                            selected,
                            move |index, _, cx| {
                                let layout = if index == 0 {
                                    Layout::Table
                                } else {
                                    Layout::Board
                                };
                                pick.update(cx, |dashboard, cx| dashboard.set_layout(layout, cx));
                            },
                        )
                        .icon_only(narrow),
                    ),
            )
    }

    /// The needs-you pill's filter is on; a click shows every task again.
    fn needs_you_chip(&self) -> Button {
        let dashboard = self.dashboard.clone();
        Button::new("needs-you-chip", "Needs you")
            .style(ButtonStyle::Subtle)
            .toggle_state(true)
            .end_icon(Icon::new(IconName::Close).size(IconSize::XSmall))
            .tooltip(Tooltip::text("Show every task"))
            .on_click(move |_, _, cx| {
                dashboard.update(cx, |dashboard, cx| dashboard.set_needs_you(false, cx))
            })
    }

    fn key_button(
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        command: &str,
        primary: bool,
    ) -> ButtonLike {
        let style = if primary {
            ButtonStyle::Tinted(TintColor::Accent)
        } else {
            ButtonStyle::Filled
        };
        ButtonLike::new(id).style(style).child(
            h_flex()
                .gap_2()
                .px_1()
                .child(Icon::from_path(format!("icons/lucide/{icon}.svg")).size(IconSize::Small))
                .child(Label::new(label))
                .child(Kbd::new(shortcut_of(command).unwrap_or_default())),
        )
    }

    fn render_body(&self, content: MainContent, cx: &mut Context<Self>) -> AnyElement {
        match content {
            MainContent::Loading => div().into_any_element(),
            MainContent::Error { reason, refused } => {
                let body = if refused {
                    "The daemon rejected the token. Check NATAMUX_TOKEN, then try again."
                } else {
                    "The app cannot reach the daemon. It keeps trying by itself."
                };
                EmptyState::error("tasks-error", "Could not load tasks", reason)
                    .body(body)
                    .action(
                        Button::new("tasks-retry", "Retry")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dashboard
                                    .update(cx, |dashboard, cx| dashboard.watch(cx))
                            })),
                    )
                    .into_any_element()
            }
            MainContent::NoTasks => EmptyState::new("tasks-empty", "list-todo", "No tasks yet")
                .body("Add a task to the backlog, or start one now in its own workspace.")
                .action(
                    Self::key_button("tasks-empty-add", "Add task", "plus", "task.new", true)
                        .on_click(
                            cx.listener(|this, _, window, cx| this.focus_new_task(window, cx)),
                        ),
                )
                .action(
                    Self::key_button(
                        "tasks-empty-start",
                        "Start a task",
                        "square-terminal",
                        "workspace.new",
                        false,
                    )
                    .on_click(cx.listener(|_, _, _, cx| cx.emit(StartTask(None)))),
                )
                .into_any_element(),
            MainContent::FilterMiss { title, body } => {
                EmptyState::new("tasks-miss", "search-x", title)
                    .body(body)
                    .action(
                        Button::new("tasks-clear-filter", "Clear filter")
                            .style(ButtonStyle::Filled)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.dashboard.update(cx, |dashboard, cx| {
                                    dashboard.set_filter(String::new(), cx);
                                    dashboard.set_state_filter(None, cx);
                                })
                            })),
                    )
                    .into_any_element()
            }
            MainContent::Tasks => match self.dashboard.read(cx).view().layout {
                Layout::Table => self.table.clone().into_any_element(),
                Layout::Board => self.board.clone().into_any_element(),
            },
        }
    }
}

impl Render for TaskView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let (content, board) = {
            let dashboard = self.dashboard.read(cx);
            let data = &dashboard.data;
            let down = match &data.link {
                Link::Down { reason, retrying } => Some((reason.as_str(), *retrying)),
                _ => None,
            };
            let content = match main_content(
                data.loaded,
                down,
                data.tasks.len(),
                self.shown_ids.len(),
                &dashboard.filter,
                dashboard.state_filter,
            ) {
                MainContent::FilterMiss { body, .. }
                    if dashboard.needs_you && dashboard.filter.trim().is_empty() =>
                {
                    MainContent::FilterMiss {
                        title: "No tasks need you".into(),
                        body,
                    }
                }
                content => content,
            };
            (content, dashboard.view().layout == Layout::Board)
        };
        let loading = content == MainContent::Loading;
        // The board scrolls sideways edge to edge; everything else has the page margin.
        let padded = !(board && content == MainContent::Tasks);
        let body = self.render_body(content, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .on_action(cx.listener(|this, _: &ShowTable, _, cx| this.set_layout(Layout::Table, cx)))
            .on_action(cx.listener(|this, _: &ShowBoard, _, cx| this.set_layout(Layout::Board, cx)))
            .child(self.render_header(cx))
            .child(self.quick_add.clone())
            .child(
                div()
                    .id("task-view-content")
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .flex_col()
                    .when(padded, |body| body.px_4().pb_4())
                    .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                        this.set_pointer_inside(*hovered, cx)
                    }))
                    .child(
                        Loading::new("tasks-loading", loading, SkeletonList::new(8, 40.))
                            .content(body),
                    ),
            )
    }
}
