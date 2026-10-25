//! All tasks in main (the main area of `Dashboard.tsx`, ui-spec 4.29): the view header with the
//! count, the state filter, Show finished and Table / Board / Progress (`task_view_header.rs`);
//! then the table, the board or the progress, or the skeleton, error or empty
//! state. The rows keep one order while the pointer is over
//! them and 2 s after a key move (`frozen_order`).

use std::time::Duration;

use gpui::{Entity, FocusHandle, Focusable, Subscription};
use natamux_proto::tasks::task::Task;
use ui::prelude::*;

use super::add_subtask_dialog;
use super::frozen_order::{HOLD_MS, freeze_order, is_frozen};
use super::task_attention::{filter_tasks, next_row_change_ms};
use super::task_board::TaskBoard;
use super::task_sort::sort_tasks;
use super::task_table::{KeyMoved, TaskTable};
use super::task_view_rules::{MainContent, main_content};
use crate::app::clock::follow_labels;
use crate::app::commands::shortcut_of;
use crate::app::commands_stages::ShowTasksProgress;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::Link;
use crate::app::settings_reads;
use crate::app::shortcuts::{NewTask, NewWorkspace, ShowBoard, ShowTable};
use crate::app::view_state::Layout;
use crate::components::button_style::{action_button, button_icon, key_hint, quiet_button};
use crate::components::empty_state::EmptyState;
use crate::components::skeleton::{Loading, SkeletonList};
use crate::stages::progress_board::ProgressBoard;

pub use crate::app::clock::now_ms;

#[path = "task_view_header.rs"]
mod header;

pub struct TaskView {
    dashboard: Entity<Dashboard>,
    table: Entity<TaskTable>,
    board: Entity<TaskBoard>,
    progress: Entity<ProgressBoard>,
    /// The width of the view header: the main width that picks columns and labels.
    main_width: Entity<Option<f32>>,
    pointer_inside: bool,
    held_at: Option<i64>,
    /// The order last shown, and what it was sorted and filtered by.
    shown_ids: Vec<String>,
    shown_key: String,
    /// When a row shown next changes a time label, in Unix ms, and the timer that draws it.
    next_change: Option<i64>,
    clock: Option<gpui::Task<()>>,
    _hold: Option<gpui::Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl Focusable for TaskView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.dashboard.read(cx).view().layout {
            Layout::Table => self.table.focus_handle(cx),
            Layout::Board => self.board.focus_handle(cx),
            Layout::Progress => self.progress.focus_handle(cx),
        }
    }
}

impl TaskView {
    pub fn new(dashboard: Entity<Dashboard>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        add_subtask_dialog::init(&dashboard, cx);
        // Made here, before the details panel and the progress layout that read it.
        settings_reads::init(&dashboard, window, cx);
        let table = cx.new(|cx| TaskTable::new(dashboard.clone(), cx));
        let board = cx.new(|cx| TaskBoard::new(dashboard.clone(), cx));
        let progress = cx.new(|cx| ProgressBoard::new(dashboard.clone(), cx));
        let main_width = cx.new(|_| None);
        let subscriptions = vec![
            cx.observe(&dashboard, |this, _, cx| this.refresh(cx)),
            cx.observe(&main_width, |this, _, cx| this.refresh(cx)),
            cx.subscribe(&table, |this, _, _: &KeyMoved, cx| this.hold(cx)),
        ];
        let mut view = TaskView {
            dashboard,
            table,
            board,
            progress,
            main_width,
            pointer_inside: false,
            held_at: None,
            shown_ids: Vec::new(),
            shown_key: String::new(),
            next_change: None,
            clock: None,
            _hold: None,
            _subscriptions: subscriptions,
        };
        view.refresh(cx);
        view
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
            let visible = filter_tasks(&data.tasks, dashboard.state_filter, &dashboard.filter);
            let sorted = sort_tasks(&visible, dashboard.sort, &data.diff_stats, &data.usage);
            let key = format!(
                "{:?}|{:?}|{}",
                dashboard.sort, dashboard.state_filter, dashboard.filter
            );
            (sorted, key)
        };
        let now = now_ms();
        let frozen = is_frozen(
            self.pointer_inside,
            self.held_at,
            now,
            key == self.shown_key,
        );
        let ordered: Vec<Task> = if frozen {
            freeze_order(&self.shown_ids, &sorted, |task| task.id.as_str())
        } else {
            sorted
        };
        self.shown_ids = ordered.iter().map(|task| task.id.clone()).collect();
        self.shown_key = key;
        self.next_change = next_row_change_ms(&ordered, now).map(|wait| now + wait);
        self.clock = None;
        let width = *self.main_width.read(cx);
        // Only the layout on screen gets the rows. A switch of layout changes the dashboard,
        // which refreshes again for the new one.
        match self.dashboard.read(cx).view().layout {
            Layout::Table => self
                .table
                .update(cx, |table, cx| table.set_rows(ordered, width, cx)),
            Layout::Board => self
                .board
                .update(cx, |board, cx| board.set_rows(ordered, cx)),
            Layout::Progress => self.progress.update(cx, |board, cx| {
                board.set_rows(ordered, cx);
                board.read_settings(cx);
            }),
        }
        cx.notify();
    }

    /// A time label of the layout on screen changed by the clock alone: it draws again, and the
    /// order and the next wait are made again.
    fn labels_changed(&mut self, cx: &mut Context<Self>) {
        self.clock = None;
        self.refresh(cx);
        match self.dashboard.read(cx).view().layout {
            Layout::Table => self.table.update(cx, |_, cx| cx.notify()),
            Layout::Board => self.board.update(cx, |_, cx| cx.notify()),
            Layout::Progress => self.progress.update(cx, |_, cx| cx.notify()),
        }
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

    fn key_button(
        id: &'static str,
        label: &'static str,
        icon: &'static str,
        command: &str,
    ) -> Button {
        action_button(id, label)
            .start_icon(button_icon(icon))
            .key_binding(key_hint(shortcut_of(command).unwrap_or_default(), false))
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
                    .action(quiet_button("tasks-retry", "Retry").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.dashboard
                                .update(cx, |dashboard, cx| dashboard.watch(cx))
                        },
                    )))
                    .into_any_element()
            }
            MainContent::NoTasks => EmptyState::new("tasks-empty", "list-todo", "No tasks yet")
                .body("Add a task to the backlog, or start one now in its own workspace.")
                .action(
                    Self::key_button("tasks-empty-add", "Add task", "plus", "task.new").on_click(
                        |_, window, cx| {
                            window.dispatch_action(Box::new(NewTask), cx);
                        },
                    ),
                )
                .action(
                    Self::key_button(
                        "tasks-empty-start",
                        "Start a task",
                        "square-terminal",
                        "workspace.new",
                    )
                    .on_click(|_, window, cx| {
                        window.dispatch_action(Box::new(NewWorkspace), cx);
                    }),
                )
                .into_any_element(),
            MainContent::FilterMiss { title, body } => {
                EmptyState::new("tasks-miss", "search-x", title)
                    .body(body)
                    .action(
                        action_button("tasks-clear-filter", "Clear filter").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.dashboard.update(cx, |dashboard, cx| {
                                    dashboard.set_filter(String::new(), cx);
                                    dashboard.set_state_filter(None, cx);
                                })
                            },
                        )),
                    )
                    .into_any_element()
            }
            MainContent::Tasks => match self.dashboard.read(cx).view().layout {
                Layout::Table => self.table.clone().into_any_element(),
                Layout::Board => self.board.clone().into_any_element(),
                Layout::Progress => self.progress.clone().into_any_element(),
            },
        }
    }
}

impl Render for TaskView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let wait = self.next_change.map(|at| at - now_ms());
        follow_labels(&mut self.clock, wait, cx, Self::labels_changed);
        let content = {
            let dashboard = self.dashboard.read(cx);
            let data = &dashboard.data;
            let down = match &data.link {
                Link::Down { reason, retrying } => Some((reason.as_str(), *retrying)),
                _ => None,
            };
            main_content(
                data.loaded,
                down,
                data.tasks.len(),
                self.shown_ids.len(),
                &dashboard.filter,
                dashboard.state_filter,
            )
        };
        let loading = content == MainContent::Loading;
        // The table sits flush, the board scrolls sideways edge to edge and the progress pads
        // itself; the empty and error states keep the page margin.
        let padded = content != MainContent::Tasks;
        let body = self.render_body(content, cx);
        v_flex()
            .size_full()
            .min_h_0()
            .on_action(cx.listener(|this, _: &ShowTable, _, cx| this.set_layout(Layout::Table, cx)))
            .on_action(cx.listener(|this, _: &ShowBoard, _, cx| this.set_layout(Layout::Board, cx)))
            .on_action(cx.listener(|this, _: &ShowTasksProgress, _, cx| {
                this.set_layout(Layout::Progress, cx)
            }))
            .child(self.render_header(cx))
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
                        Loading::new("tasks-loading", loading, SkeletonList::new(12, 28.))
                            .content(body),
                    ),
            )
    }
}
