//! The dashboard entity: the live data of the feed, what the user picked (selection, filter,
//! sort, the open task), the saved view state and the toasts. Views read it and call its
//! actions; each action makes the daemon call of the React screen (`useTaskMutations`) and
//! toasts the result.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use futures::StreamExt;
use gpui::EventEmitter;
use natamux_client::DashboardEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::dashboard_data::{BulkRuns, DashboardData, Link};
use super::source::{Feed, Rows, Source};
use super::view_rules::{seen_after_list, toggle_pin, with_seen};
use super::view_state::{self, Layout, ViewState};
use crate::components::toast_rules::{ToastTone, Toasts, toast_duration};
use crate::sidebar::sidebar_tree::with_fold;
use crate::tasks::task_sort::{SortKey, TaskSort, next_sort};

/// What views and Root follow besides `cx.notify()`.
#[derive(Clone, Debug)]
pub enum DashboardChange {
    /// The daemon sent a new task list.
    Tasks,
    /// The user opened a task in main.
    Opened(Box<Task>),
    /// Main shows All tasks again.
    BackToList,
}

/// The view file is written at most this often, off the main thread.
const SAVE_AFTER: Duration = Duration::from_millis(300);

pub struct Dashboard {
    source: Option<Arc<Source>>,
    pub data: DashboardData,
    rows: Option<Rows>,
    _feed: Option<gpui::Task<()>>,
    view: ViewState,
    view_folder: Option<PathBuf>,
    view_dirty: bool,
    saving: Option<gpui::Task<()>>,
    events_reading: BTreeSet<String>,
    pub toasts: Toasts,
    pub selected: Option<String>,
    pub open_task: Option<String>,
    /// The sidebar filter text.
    pub filter: String,
    pub state_filter: Option<TaskState>,
    /// The needs-you pill's filter: every task that needs the user, of any state.
    pub needs_you: bool,
    pub sort: TaskSort,
    pub bulk: BulkRuns,
}

impl EventEmitter<DashboardChange> for Dashboard {}

impl Dashboard {
    /// `source`: the daemon or the fake, or why the app has none.
    pub fn new(source: Result<Arc<Source>, String>, cx: &mut Context<Self>) -> Self {
        // The fake data reuses real ids (T-0146, natamux): its seen states, pins and folds must
        // not reach the user's file.
        let fake = matches!(source.as_deref(), Ok(Source::FakeDaemon));
        let view_folder = if fake { None } else { view_state::folder() };
        let view = view_folder
            .as_deref()
            .map(view_state::load)
            .unwrap_or_default();
        let mut data = DashboardData::default();
        if let Err(reason) = &source {
            data.link = Link::Down {
                reason: reason.clone(),
                retrying: false,
            };
        }
        let mut dashboard = Dashboard {
            source: source.ok(),
            data,
            rows: None,
            _feed: None,
            view,
            view_folder,
            view_dirty: false,
            saving: None,
            events_reading: BTreeSet::new(),
            toasts: Toasts::default(),
            selected: None,
            open_task: None,
            filter: String::new(),
            state_filter: None,
            needs_you: false,
            sort: None,
            bulk: BulkRuns::default(),
        };
        dashboard.watch(cx);
        dashboard
    }

    pub fn source(&self) -> Option<&Arc<Source>> {
        self.source.as_ref()
    }

    pub fn view(&self) -> &ViewState {
        &self.view
    }

    /// A fixed view that is never saved: the shots of `--shot` must not read or change the
    /// user's file.
    pub fn use_view(&mut self, view: ViewState, cx: &mut Context<Self>) {
        self.view = view;
        self.view_folder = None;
        cx.notify();
    }

    /// Follows the feed again from the start: Retry of "Could not load tasks".
    pub fn watch(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        let Feed { mut events, rows } = source.watch_dashboard();
        self.rows = Some(rows);
        self.data.link = Link::Loading;
        self._feed = Some(cx.spawn(async move |this, cx| {
            while let Some(event) = events.next().await {
                if this.update(cx, |this, cx| this.apply(event, cx)).is_err() {
                    break;
                }
            }
        }));
        cx.notify();
    }

    fn apply(&mut self, event: DashboardEvent, cx: &mut Context<Self>) {
        let tasks_changed = matches!(event, DashboardEvent::Tasks(_));
        if let Some(notice) = self.data.apply(event) {
            self.toast(&notice.text, notice.tone, None, cx);
        }
        if tasks_changed {
            crate::perf::report::mark_first_data();
            self.after_tasks(cx);
        }
        cx.notify();
    }

    fn after_tasks(&mut self, cx: &mut Context<Self>) {
        let gone =
            |id: &Option<String>| id.as_deref().is_some_and(|id| self.data.task(id).is_none());
        if gone(&self.selected) {
            self.selected = None;
        }
        if gone(&self.open_task) {
            self.back(cx);
        }
        let seen = seen_after_list(&self.view.seen, &self.data.tasks, self.open_task.as_deref());
        if seen != self.view.seen {
            self.view.seen = seen;
            self.save_view(cx);
        }
        self.read_events(cx);
        cx.emit(DashboardChange::Tasks);
    }

    /// The question or failure reason of each attention card (the web's `LOG_KEYS` reads).
    fn read_events(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.source.clone() else {
            return;
        };
        for (id, updated_at) in self.data.events_to_read() {
            if !self.events_reading.insert(id.clone()) {
                continue;
            }
            let answer = source.list_events(&id);
            cx.spawn(async move |this, cx| {
                let events = answer.await;
                this.update(cx, |this, cx| {
                    this.events_reading.remove(&id);
                    // A card without its question still shows; the next change reads again.
                    if let Ok(events) = events {
                        this.data.set_events(&id, &updated_at, events);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// The tasks whose terminal rows the sidebar shows (`GET /tasks/:id/terminals`).
    pub fn show_terminals(&self, task_ids: impl IntoIterator<Item = String>) {
        if let Some(rows) = &self.rows {
            rows.show(task_ids);
        }
    }

    // Toasts

    pub fn toast(
        &mut self,
        text: &str,
        tone: ToastTone,
        detail: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.toasts.push(text, tone, detail, Vec::new());
        let id = self.toasts.toasts.last().map(|toast| toast.id);
        if let (Some(id), Some(ms)) = (id, toast_duration(tone, false)) {
            cx.spawn(async move |this, cx| {
                cx.background_executor()
                    .timer(Duration::from_millis(u64::from(ms)))
                    .await;
                this.update(cx, |this, cx| this.dismiss_toast(id, cx)).ok();
            })
            .detach();
        }
        cx.notify();
    }

    pub fn dismiss_toast(&mut self, id: u64, cx: &mut Context<Self>) {
        self.toasts.dismiss(id);
        cx.notify();
    }

    // What the user picked

    pub fn select(&mut self, id: Option<String>, cx: &mut Context<Self>) {
        self.selected = id;
        cx.notify();
    }

    /// Opens the task in main and records the state the user now saw.
    pub fn open(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(task) = self.data.task(id).cloned() else {
            return;
        };
        self.selected = Some(task.id.clone());
        self.open_task = Some(task.id.clone());
        self.view.seen = with_seen(&self.view.seen, &[(&task.id, task.state)], true);
        self.save_view(cx);
        cx.emit(DashboardChange::Opened(Box::new(task)));
        cx.notify();
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        if self.open_task.take().is_some() {
            cx.emit(DashboardChange::BackToList);
            cx.notify();
        }
    }

    pub fn set_filter(&mut self, text: String, cx: &mut Context<Self>) {
        self.filter = text;
        cx.notify();
    }

    /// A chip filters to its state; the same chip again shows all.
    pub fn toggle_state_filter(&mut self, state: TaskState, cx: &mut Context<Self>) {
        self.state_filter = (self.state_filter != Some(state)).then_some(state);
        self.needs_you = false;
        cx.notify();
    }

    pub fn set_state_filter(&mut self, state: Option<TaskState>, cx: &mut Context<Self>) {
        self.state_filter = state;
        self.needs_you = false;
        cx.notify();
    }

    /// The needs-you pill and the status bar's "N need you".
    pub fn set_needs_you(&mut self, on: bool, cx: &mut Context<Self>) {
        self.needs_you = on;
        self.state_filter = None;
        cx.notify();
    }

    pub fn sort_by(&mut self, key: SortKey, cx: &mut Context<Self>) {
        self.sort = next_sort(self.sort, key);
        cx.notify();
    }

    // The saved view

    pub fn set_layout(&mut self, layout: Layout, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.layout = layout);
    }

    pub fn toggle_show_finished(&mut self, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.show_finished = !view.show_finished);
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.sidebar_open = !view.sidebar_open);
    }

    pub fn set_sidebar_open(&mut self, open: bool, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.sidebar_open = open);
    }

    pub fn set_sidebar_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.set_sidebar_width(width));
    }

    pub fn set_fold(&mut self, key: &str, open: bool, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.folds = with_fold(&view.folds, key, open));
    }

    pub fn toggle_pin(&mut self, id: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.pinned = toggle_pin(&view.pinned, id));
    }

    pub fn toggle_blurred(&mut self, repo: &str, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.toggle_blurred(repo));
    }

    fn update_view(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut ViewState)) {
        change(&mut self.view);
        self.save_view(cx);
        cx.notify();
    }

    /// Writes the newest view state after a short wait, one write at a time.
    fn save_view(&mut self, cx: &mut Context<Self>) {
        self.view_dirty = true;
        if self.saving.is_some() || self.view_folder.is_none() {
            return;
        }
        self.saving = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SAVE_AFTER).await;
                let next = this.update(cx, |this, _| {
                    let dirty = std::mem::take(&mut this.view_dirty);
                    let folder = this.view_folder.clone();
                    dirty.then(|| folder.map(|folder| (folder, this.view.clone())))
                });
                let Ok(Some(Some((folder, view)))) = next else {
                    break;
                };
                let saved = cx
                    .background_executor()
                    .spawn(async move { view_state::save(&folder, &view) })
                    .await;
                if let Err(error) = saved {
                    eprintln!("natamux-app: could not save the view state: {error}");
                }
            }
            this.update(cx, |this, _| this.saving = None).ok();
        }));
    }
}

#[path = "dashboard_writes.rs"]
mod writes;
