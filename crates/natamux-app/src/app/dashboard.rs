//! The dashboard entity: the live data of the feed, what the user picked (selection, filter,
//! sort, the open task), the saved view state and the toasts. Views read it and call its
//! actions; each action makes the daemon call of the React screen (`useTaskMutations`) and
//! toasts the result.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use gpui::EventEmitter;
use natamux_client::DashboardEvent;
use natamux_proto::tasks::task::{Task, TaskState};
use ui::prelude::*;

use super::dashboard_data::{BulkRuns, DashboardData, Link};
use super::settings_rules::{SettingsFocus, SettingsPage};
use super::source::{Rows, Source};
use super::view_rules::{
    ShellRequest, WorkspaceTab, request_on_open, seen_after_list, toggle_pin, with_recent,
    with_seen,
};
use super::view_state::{self, Layout, ViewState};
use crate::components::toast_rules::{ToastTone, Toasts, toast_duration};
use crate::sidebar::sidebar_tree::with_fold;
use crate::tasks::task_sort::{SortKey, TaskSort, next_sort};
use crate::terminals::pane_rules::PaneLayout;

/// What views and Root follow besides `cx.notify()`.
#[derive(Clone, Debug)]
pub enum DashboardChange {
    /// The daemon sent a new task list.
    Tasks,
    /// The user opened a task in main.
    Opened(Box<Task>),
    /// Main shows All tasks again.
    BackToList,
    /// Main shows the Agents grid.
    AgentsOpened,
    /// The files or the git state of a task's workspace changed (`workspace.dirty`).
    WorkspaceChanged(String),
    /// A task's terminals started or ended (`shells.changed`).
    ShellsChanged(String),
    /// A task's review comments changed (`review-comments.changed`).
    ReviewCommentsChanged(String),
    /// A line was added to a task's log (`task.updated`): its events, stage files and
    /// settings may have changed.
    TaskLogged(String),
    /// Undo of a rollback's toast: the task's workspace runs it as one git step.
    UndoRollback(String),
    /// The Settings page opened, or was asked for again (T-0174).
    SettingsOpened(SettingsFocus),
    /// Settings closed by its close button or Esc: main shows again.
    SettingsClosed,
}

/// The parts of the app that show terminals of tasks; the feed reads the union.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum TerminalReader {
    Sidebar,
    Workspace,
    Agents,
    /// The jump palette while it is open.
    Palette,
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
    /// Main shows the Agents grid (then no task is open).
    pub agents_open: bool,
    /// The Settings page fills the body over main (T-0174); None: closed.
    pub settings: Option<SettingsPage>,
    last_settings_page: SettingsPage,
    /// The terminal the open workspace shows and focuses once its terminals are read.
    pub shell_request: Option<ShellRequest>,
    /// The tab the open workspace shows, and the task whose commit box Commit… asked for.
    pub workspace_tab: WorkspaceTab,
    pub commit_request: Option<String>,
    pub files: tabs::FileViews,
    /// The open task and its count of files to commit (`set_open_changes`).
    pub open_changes: Option<(String, usize)>,
    /// The project last picked in Add task; Import issues starts on it.
    pub last_add_repo: Option<String>,
    pub panel_request: Option<(String, details::PanelRequest)>,
    terminal_readers: RefCell<BTreeMap<TerminalReader, BTreeSet<String>>>,
    /// The sidebar filter text.
    pub filter: String,
    pub state_filter: Option<TaskState>,
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
            agents_open: false,
            settings: None,
            last_settings_page: SettingsPage::Appearance,
            shell_request: None,
            workspace_tab: WorkspaceTab::Terminal,
            commit_request: None,
            files: tabs::FileViews::default(),
            open_changes: None,
            last_add_repo: None,
            panel_request: None,
            terminal_readers: RefCell::new(BTreeMap::new()),
            filter: String::new(),
            state_filter: None,
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

    /// Every view that shows the dashboard draws again on its notify, so an event that changes
    /// nothing notifies nobody. The three workspace events only go to their subscribers.
    fn apply(&mut self, event: DashboardEvent, cx: &mut Context<Self>) {
        super::app_log::log_status(&event, &mut self.data.link_down, &self.source, cx);
        let changed = self.data.changes(&event);
        let tasks_changed = changed && matches!(event, DashboardEvent::Tasks(_));
        match &event {
            DashboardEvent::WorkspaceChanged { task_id } => {
                cx.emit(DashboardChange::WorkspaceChanged(task_id.clone()))
            }
            DashboardEvent::ShellsChanged { task_id } => {
                cx.emit(DashboardChange::ShellsChanged(task_id.clone()))
            }
            DashboardEvent::ReviewCommentsChanged { task_id } => {
                cx.emit(DashboardChange::ReviewCommentsChanged(task_id.clone()))
            }
            DashboardEvent::TaskLogged { task_id } => {
                cx.emit(DashboardChange::TaskLogged(task_id.clone()))
            }
            _ => {}
        }
        if let Some(notice) = self.data.apply(event) {
            self.toast(&notice.text, notice.tone, None, cx);
        }
        if tasks_changed {
            crate::perf::report::mark_first_data();
            self.after_tasks(cx);
        }
        if changed {
            cx.notify();
        }
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

    /// The tasks whose terminals one part of the app shows (`GET /tasks/:id/terminals`). The
    /// feed reads those of every part; it reads nothing again while the union stays the same.
    pub fn show_terminals(
        &self,
        reader: TerminalReader,
        task_ids: impl IntoIterator<Item = String>,
    ) {
        let union: BTreeSet<String> = {
            let mut readers = self.terminal_readers.borrow_mut();
            readers.insert(reader, task_ids.into_iter().collect());
            readers.values().flatten().cloned().collect()
        };
        if let Some(rows) = &self.rows {
            rows.show(union);
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
        self.shell_request = request_on_open(
            self.open_task.as_deref(),
            self.shell_request.as_ref(),
            &task.id,
        );
        self.agents_open = false;
        self.settings = None;
        self.workspace_tab = self.tab_on_open(&task.id);
        self.open_task = Some(task.id.clone());
        self.view.seen = with_seen(&self.view.seen, &[(&task.id, task.state)], true);
        self.view.recent = with_recent(&self.view.recent, &task.id);
        self.save_view(cx);
        cx.emit(DashboardChange::Opened(Box::new(task)));
        cx.notify();
    }

    /// Opens the task's workspace on one terminal: a sidebar terminal row and the Answer button
    /// (`requestShell`). `name` None: the first agent terminal. Main shows the terminals, also
    /// when Changes or Docs covered them.
    pub fn open_shell(&mut self, id: &str, name: Option<String>, cx: &mut Context<Self>) {
        self.open(id, cx);
        if self.open_task.as_deref() == Some(id) {
            self.pick_tab(WorkspaceTab::Terminal, cx);
        }
        if name.is_some() && self.open_task.as_deref() == Some(id) {
            self.shell_request = Some(ShellRequest {
                task_id: id.to_owned(),
                name,
            });
        }
    }

    /// The open workspace takes the request meant for it, once.
    pub fn take_shell_request(&mut self, task_id: &str) -> Option<ShellRequest> {
        let ours = self
            .shell_request
            .as_ref()
            .is_some_and(|request| request.task_id == task_id);
        if ours {
            self.shell_request.take()
        } else {
            None
        }
    }

    pub fn open_agents(&mut self, cx: &mut Context<Self>) {
        self.open_task = None;
        self.shell_request = None;
        self.settings = None;
        if !self.agents_open {
            self.agents_open = true;
            cx.emit(DashboardChange::AgentsOpened);
        }
        cx.notify();
    }

    pub fn back(&mut self, cx: &mut Context<Self>) {
        let agents = std::mem::take(&mut self.agents_open);
        if self.open_task.take().is_some() || agents {
            self.shell_request = None;
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
        cx.notify();
    }

    pub fn set_state_filter(&mut self, state: Option<TaskState>, cx: &mut Context<Self>) {
        self.state_filter = state;
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

    /// The user's list of the Agents grid; None goes back to the default list.
    pub fn set_agent_panes(&mut self, ids: Option<Vec<String>>, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.agent_panes = ids);
    }

    pub fn set_pane_layout(&mut self, layout: PaneLayout, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.pane_layout = layout);
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
pub use writes::Written;

#[path = "workspace_writes.rs"]
mod workspace_writes;

#[path = "details_state.rs"]
pub mod details;

#[path = "docs_state.rs"]
mod docs_state;

#[path = "details_writes.rs"]
mod details_writes;

#[path = "git_writes.rs"]
mod git_writes;

#[path = "tab_state.rs"]
pub mod tabs;

#[path = "dashboard_session.rs"]
mod session;
