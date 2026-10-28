//! Main for an open task (docs/ui-spec.md 4.29, `features/workspaces/ChangesView.tsx` and the
//! workspace part of `Dashboard.tsx`): a task with no workspace shows the no-workspace view or,
//! once finished, its summary; one with a workspace shows the finished banner, the
//! tab row and the body of its tab: the terminal panes (or the wrap-up of a finished task),
//! Changes, History, Docs, or the open file.

use std::time::Duration;

use gpui::{Entity, Subscription};
use natamux_proto::events::task_event::StoredEvent;
use natamux_proto::projects::repo::BranchList;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};
use ui::prelude::*;

use super::workspace_rules::GitStep;
use super::wrap_up_rules::{finished_text, is_finished_state};
use crate::app::dashboard::{Dashboard, DashboardChange, TerminalReader};
use crate::app::dashboard_data::Part;
use crate::app::read_pace::ReadPace;
use crate::app::shown_cache::ShownCache;
use crate::app::view_rules::WorkspaceTab;
use crate::components::motion;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;
use crate::files::file_editor::FileEditor;
use crate::git::changes_view::ChangesView;
use crate::terminals::shell_panes::ShellPanes;
use crate::workspaces::docs_tab::DocsTab;

/// `FALLBACK_POLL_MS`: the daemon says when a workspace changed; this covers a lost event.
const POLL: Duration = Duration::from_secs(60);

/// What the view keeps of one task. The last tasks' are kept in `WorkspaceView::cache`.
#[derive(Default)]
struct Shown {
    task_id: String,
    graph: Option<GitGraph>,
    /// A failed first read of the graph, for History.
    graph_error: Option<String>,
    changes: Option<Vec<ChangeEntry>>,
    ended: Vec<EndedShell>,
    events: Option<Vec<StoredEvent>>,
    branches: Option<BranchList>,
    base: Option<String>,
    pending: Option<GitStep>,
    starting: bool,
    reopening: bool,
    /// Open a new terminal of the wrap-up: the panes show in place of it.
    shells_wanted: bool,
    shown_screen: Option<String>,
    /// The state and workspace last seen, to read again when they change.
    seen: Option<(TaskState, bool)>,
    git_reads: ReadPace,
}

pub struct WorkspaceView {
    dashboard: Entity<Dashboard>,
    panes: Entity<ShellPanes>,
    changes: Entity<ChangesView>,
    docs: Option<Entity<DocsTab>>,
    /// One editor per open file of the shown task, so each keeps its cursor, scroll and undo.
    editors: Vec<Entity<FileEditor>>,
    /// Docs or the editor tab as last drawn: a hidden one reads again when it shows.
    file_tab_shown: Option<WorkspaceTab>,
    /// The active file as last drawn: a tab picked again reads its file again.
    editor_shown: Option<String>,
    shown: Option<Shown>,
    cache: ShownCache<Shown>,
    width: Entity<Option<f32>>,
    /// A poll came while the window had no focus: it reads once when the focus comes back.
    missed_poll: bool,
    _poll: gpui::Task<()>,
    _subscriptions: Vec<Subscription>,
}

impl WorkspaceView {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let panes = cx.new(|cx| ShellPanes::new(dashboard.clone(), cx));
        let workspace = cx.weak_entity();
        let changes_view = cx.new(|cx| ChangesView::new(dashboard.clone(), workspace, cx));
        let width = cx.new(|_| None);
        let changes = cx.subscribe(&dashboard, |view, _, change: &DashboardChange, cx| {
            view.on_change(change, cx)
        });
        let observe = cx.observe(&width, |_, _, cx| cx.notify());
        // Root caches this view: the task, its terminals and their lines come from the dashboard.
        // The terminal panes and the docs tab inside it have no observer of their own.
        let parts = &[
            Part::Tasks,
            Part::Terminals,
            Part::Activity,
            Part::Preparing,
            Part::View,
        ];
        let follow = Dashboard::redraw_on(&dashboard, parts, cx);
        let poll = cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let read = |view: &mut Self, cx: &mut Context<Self>| {
                    if motion::window_active(cx) {
                        view.read_polled(cx);
                    } else {
                        view.missed_poll = true;
                    }
                };
                if view.update(cx, read).is_err() {
                    break;
                }
            }
        });
        WorkspaceView {
            dashboard,
            panes,
            changes: changes_view,
            docs: None,
            editors: Vec::new(),
            file_tab_shown: None,
            editor_shown: None,
            shown: None,
            cache: ShownCache::default(),
            width,
            missed_poll: false,
            _poll: poll,
            _subscriptions: vec![changes, observe, follow],
        }
    }

    fn read_polled(&mut self, cx: &mut Context<Self>) {
        self.read_git(cx);
        self.refresh_file_tabs(cx);
    }

    /// The window has focus again: the poll it skipped reads once now.
    pub fn read_missed(&mut self, cx: &mut Context<Self>) {
        if std::mem::take(&mut self.missed_poll) {
            self.read_polled(cx);
        }
    }

    /// The panes, for `--gate2`.
    pub fn panes(&self) -> &Entity<ShellPanes> {
        &self.panes
    }

    /// The Changes tab, for the `--shot` states.
    pub fn changes_view(&self) -> &Entity<ChangesView> {
        &self.changes
    }

    /// The git step that runs now, for the dialogs that must wait for it.
    pub fn git_pending(&self) -> Option<GitStep> {
        self.shown.as_ref()?.pending
    }

    /// The changed files last read; None before the first read.
    pub fn current_changes(&self) -> Option<&[ChangeEntry]> {
        self.shown.as_ref()?.changes.as_deref()
    }

    fn task(&self, cx: &App) -> Option<Task> {
        let id = &self.shown.as_ref()?.task_id;
        self.dashboard.read(cx).data.task(id).cloned()
    }

    fn on_change(&mut self, change: &DashboardChange, cx: &mut Context<Self>) {
        let open = |id: &str, view: &Self| view.shown.as_ref().is_some_and(|s| s.task_id == id);
        match change {
            DashboardChange::Opened(task) => self.show(&task.id, cx),
            DashboardChange::BackToList | DashboardChange::AgentsOpened => self.hide(cx),
            DashboardChange::ReviewCommentsChanged(id) if open(id, self) => {
                self.changes
                    .update(cx, |changes, cx| changes.read_comments(cx));
            }
            DashboardChange::WorkspaceChanged(id) if open(id, self) => {
                self.changes
                    .update(cx, |changes, cx| changes.read_comments(cx));
                self.read_git(cx);
                self.refresh_file_tabs(cx);
            }
            DashboardChange::TaskLogged(id) if open(id, self) => self.refresh_docs(cx),
            DashboardChange::ShellsChanged(id) if open(id, self) => self.read_ended(cx),
            DashboardChange::WorkspaceChanged(id) | DashboardChange::TaskLogged(id) => {
                self.cache.mark_stale(id)
            }
            DashboardChange::Tasks => self.after_tasks(cx),
            DashboardChange::UndoRollback(id) => self.undo_rollback(id, cx),
            _ => {}
        }
    }

    fn show(&mut self, task_id: &str, cx: &mut Context<Self>) {
        if self.shown.as_ref().is_some_and(|s| s.task_id == task_id) {
            return;
        }
        self.keep_shown();
        let stale = self.cache.is_stale(task_id);
        let mut shown = self.cache.take(task_id).unwrap_or_else(|| Shown {
            task_id: task_id.to_owned(),
            ..Shown::default()
        });
        // Drawn from what was kept; `after_tasks` reads again (a read that ran when the task
        // left was dropped, so its pace must not wait for it).
        shown.git_reads.reset();
        shown.seen = None;
        self.shown = Some(shown);
        self.panes
            .update(cx, |panes, cx| panes.set_task(Some(task_id.to_owned()), cx));
        self.sync_changes(cx);
        self.dashboard
            .read(cx)
            .show_terminals(TerminalReader::Workspace, [task_id.to_owned()]);
        if stale {
            // Asked first, so the one read of `after_tasks` waits for this one.
            self.read_git_fresh(cx);
        }
        self.after_tasks(cx);
        cx.notify();
    }

    /// Keeps the shown task for a switch back. The answer of a git step or a start that was on
    /// its way is dropped (it finds another task shown), so its mark would stay for ever: it is
    /// cleared here. The read data and the user's choices (base, tab screen) stay.
    fn keep_shown(&mut self) {
        let Some(mut shown) = self.shown.take() else {
            return;
        };
        shown.pending = None;
        shown.starting = false;
        shown.reopening = false;
        let id = shown.task_id.clone();
        self.cache.put(id, shown);
    }

    fn hide(&mut self, cx: &mut Context<Self>) {
        self.keep_shown();
        self.docs = None;
        self.editors.clear();
        self.panes.update(cx, |panes, cx| panes.set_task(None, cx));
        self.dashboard
            .read(cx)
            .show_terminals(TerminalReader::Workspace, []);
        cx.notify();
    }

    /// A new list: read again what the task's new state needs; say once that it finished.
    fn after_tasks(&mut self, cx: &mut Context<Self>) {
        let dashboard = self.dashboard.read(cx);
        self.cache.retain(|id| dashboard.data.task(id).is_some());
        let Some(task) = self.task(cx) else {
            return;
        };
        let now = (task.state, task.workspace.is_some());
        let Some(shown) = self.shown.as_mut() else {
            return;
        };
        let before = shown.seen.replace(now);
        if before == Some(now) {
            return;
        }
        let finished = is_finished_state(task.state);
        if before.is_some_and(|(state, _)| !is_finished_state(state)) && finished {
            toast(&finished_text(&task), ToastTone::Info, cx);
        }
        if now.1 {
            self.read_git(cx);
        } else {
            self.read_branches(&task, cx);
        }
        if finished {
            self.read_ended(cx);
            if !now.1 {
                self.read_events(cx);
            }
        }
        cx.notify();
    }
}

impl Render for WorkspaceView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(task) = self.task(cx) else {
            return div().size_full().into_any_element();
        };
        let Some(shown) = self.shown.take() else {
            return div().size_full().into_any_element();
        };
        let element = if task.workspace.is_none() {
            self.render_no_workspace(task, &shown, cx)
        } else {
            self.render_workspace(task, &shown, window, cx)
        };
        self.shown = Some(shown);
        element
    }
}

#[path = "workspace_reads.rs"]
mod reads;

#[path = "workspace_parts.rs"]
mod parts;

#[path = "workspace_tabs.rs"]
mod tabs;

#[path = "workspace_file_tabs.rs"]
mod file_tabs;

#[cfg(test)]
#[path = "workspace_view_tests.rs"]
mod tests;
