//! The task list: live from the daemon, filtered by the text box above it.

use editor::{Editor, EditorEvent};
use futures::StreamExt;
use futures::channel::mpsc::UnboundedReceiver;
use gpui::{
    App, Entity, EventEmitter, FocusHandle, Focusable, ScrollStrategy, UniformListScrollHandle,
    actions, uniform_list,
};
use natamux_client::{LinkStatus, TaskListEvent};
use natamux_proto::tasks::task::{Task, TaskState};
use ui::{ListItem, prelude::*};

use super::task_filter::{keep_selection, visible_tasks};

actions!(natamux, [SelectNextTask, SelectPreviousTask, OpenTask]);

/// What the list knows of the daemon.
enum Link {
    Loading,
    Live,
    /// The daemon could not be used, and why. `retrying`: the client tries again by itself.
    Down {
        reason: String,
        retrying: bool,
    },
}

pub struct TaskList {
    filter: Entity<Editor>,
    link: Link,
    tasks: Vec<Task>,
    shown: Vec<Task>,
    selected: Option<usize>,
    scroll: UniformListScrollHandle,
    focus: FocusHandle,
    /// Open the first task as soon as there is one (`--fake-daemon`, for screenshots).
    pub open_first: bool,
}

/// The person picked a task to see.
pub struct TaskOpened(pub Task);

impl EventEmitter<TaskOpened> for TaskList {}

/// The daemon sent a new list. Views that show a task take its new state from it.
pub struct TasksChanged(pub Vec<Task>);

impl EventEmitter<TasksChanged> for TaskList {}

impl TaskList {
    /// `events`: the client's live list, or None with the reason the app has no client.
    pub fn new(
        events: Result<UnboundedReceiver<TaskListEvent>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_placeholder_text("Filter tasks", window, cx);
            editor
        });
        cx.subscribe(&filter, |list, _, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                list.refilter(cx);
            }
        })
        .detach();
        let link = match events {
            Ok(mut events) => {
                cx.spawn(async move |list, cx| {
                    while let Some(event) = events.next().await {
                        if list.update(cx, |list, cx| list.apply(event, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                Link::Loading
            }
            Err(reason) => Link::Down {
                reason,
                retrying: false,
            },
        };
        TaskList {
            filter,
            link,
            tasks: Vec::new(),
            shown: Vec::new(),
            selected: None,
            scroll: UniformListScrollHandle::new(),
            focus: cx.focus_handle(),
            open_first: false,
        }
    }

    pub fn filter(&self) -> &Entity<Editor> {
        &self.filter
    }

    /// A task of the daemon's list, once the list came.
    pub fn task(&self, id: &str) -> Option<&Task> {
        self.tasks.iter().find(|task| task.id == id)
    }

    fn apply(&mut self, event: TaskListEvent, cx: &mut Context<Self>) {
        match event {
            TaskListEvent::Tasks(tasks) => {
                self.link = Link::Live;
                self.tasks = tasks;
                self.refilter(cx);
                cx.emit(TasksChanged(self.tasks.clone()));
                if std::mem::take(&mut self.open_first)
                    && let Some(first) = self.shown.first()
                {
                    cx.emit(TaskOpened(first.clone()));
                }
            }
            TaskListEvent::Status(LinkStatus::Connecting) => {}
            TaskListEvent::Status(LinkStatus::Live) => self.link = Link::Live,
            TaskListEvent::Status(LinkStatus::Reconnecting(error)) => {
                self.link = Link::Down {
                    reason: error.to_string(),
                    retrying: true,
                };
            }
            TaskListEvent::Status(LinkStatus::Refused) => {
                self.link = Link::Down {
                    reason: natamux_client::ClientError::Unauthorized.to_string(),
                    retrying: false,
                };
            }
        }
        cx.notify();
    }

    fn refilter(&mut self, cx: &mut Context<Self>) {
        let query = self.filter.read(cx).text(cx);
        let selected_id = self
            .selected
            .and_then(|index| self.shown.get(index))
            .map(|task| task.id.clone());
        self.shown = visible_tasks(&self.tasks, &query);
        self.selected = keep_selection(
            &self.shown,
            selected_id.as_deref(),
            self.selected.unwrap_or(0),
        );
        cx.notify();
    }

    fn select(&mut self, index: usize, cx: &mut Context<Self>) {
        if index < self.shown.len() {
            self.selected = Some(index);
            self.scroll.scroll_to_item(index, ScrollStrategy::Top);
            cx.notify();
        }
    }

    fn select_next(&mut self, _: &SelectNextTask, _: &mut Window, cx: &mut Context<Self>) {
        let next = self.selected.map_or(0, |index| index + 1);
        self.select(next.min(self.shown.len().saturating_sub(1)), cx);
    }

    fn select_previous(&mut self, _: &SelectPreviousTask, _: &mut Window, cx: &mut Context<Self>) {
        self.select(self.selected.map_or(0, |index| index.saturating_sub(1)), cx);
    }

    fn open(&mut self, _: &OpenTask, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(task) = self.selected.and_then(|index| self.shown.get(index)) {
            cx.emit(TaskOpened(task.clone()));
        }
    }

    fn render_row(&self, index: usize, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let task = &self.shown[index];
        let state = Label::new(task.state.label())
            .size(LabelSize::Small)
            .color(state_color(task.state));
        ListItem::new(index)
            .spacing(ui::ListItemSpacing::Sparse)
            .toggle_state(self.selected == Some(index))
            .on_click(cx.listener(move |list, _, _, cx| {
                list.select(index, cx);
                if let Some(task) = list.shown.get(index) {
                    cx.emit(TaskOpened(task.clone()));
                }
            }))
            .child(
                v_flex()
                    .w_full()
                    .min_w_0()
                    .child(Label::new(task.title.clone()).truncate())
                    .child(
                        h_flex()
                            .gap_2()
                            .child(
                                Label::new(task.id.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted),
                            )
                            .child(state)
                            .child(
                                Label::new(task.stage.clone())
                                    .size(LabelSize::Small)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    ),
            )
    }

    fn render_note(&self) -> Option<impl IntoElement + use<>> {
        let note = match &self.link {
            Link::Loading => "Loading tasks…".to_string(),
            Link::Down { reason, retrying } => {
                let again = if *retrying { " Trying again…" } else { "" };
                format!("{reason}{again}")
            }
            Link::Live if self.tasks.is_empty() => "No tasks yet.".to_string(),
            Link::Live if self.shown.is_empty() => "No task matches the filter.".to_string(),
            Link::Live => return None,
        };
        let color = match self.link {
            Link::Down { .. } => Color::Error,
            _ => Color::Muted,
        };
        Some(div().p_2().child(Label::new(note).color(color)))
    }
}

fn state_color(state: TaskState) -> Color {
    match state {
        TaskState::NeedsInput | TaskState::Review => Color::Warning,
        TaskState::Failed | TaskState::Blocked => Color::Error,
        TaskState::Running => Color::Accent,
        TaskState::Done => Color::Success,
        _ => Color::Muted,
    }
}

impl Focusable for TaskList {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.focus_handle(cx)
    }
}

impl Render for TaskList {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let count = self.shown.len();
        v_flex()
            .key_context("TaskList")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::open))
            .size_full()
            .gap_1()
            .p_2()
            .bg(colors.panel_background)
            .child(
                div()
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_variant)
                    .bg(colors.editor_background)
                    .child(self.filter.clone()),
            )
            .children(self.render_note())
            .child(
                uniform_list(
                    "tasks",
                    count,
                    cx.processor(|list, range: std::ops::Range<usize>, _window, cx| {
                        range
                            .map(|index| list.render_row(index, cx))
                            .collect::<Vec<_>>()
                    }),
                )
                .track_scroll(&self.scroll)
                .flex_grow_1(),
            )
    }
}
