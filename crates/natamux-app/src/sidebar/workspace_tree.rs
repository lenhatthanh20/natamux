//! The tree of workspaces in the sidebar (docs/ui-spec.md 4.9, `WorkspaceList.tsx`). The whole
//! tree is one tab stop: arrows, Home and End move, Left and Right fold and unfold, Enter opens,
//! F2 renames, Shift+F10 opens the row menu, Delete asks to remove the workspace, and letters jump
//! to a row by title. What it shows comes from `tree_model`; `workspace_row` and `project_header`
//! draw the lines.

use std::collections::BTreeMap;

use gpui::{
    Entity, FocusHandle, Focusable, KeyDownEvent, ListAlignment, ListState, Pixels, Point,
    Subscription, list,
};
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use crate::app::clock::follow_labels;
use crate::app::dashboard::{Dashboard, TerminalReader};
use crate::app::shortcuts::{
    Back, CollapseRow, ExpandRow, OpenRowMenu, OpenSelected, RemoveRow, RenameRow, SelectFirst,
    SelectLast, SelectNext, SelectPrevious,
};
use crate::sidebar::sidebar_text::{type_ahead_target, typed_word};
use crate::sidebar::tree_entries::{EntryAction, TreeKey, move_target, tree_key};
use crate::sidebar::tree_model::{Line, TreeInput, TreeModel};
use crate::sidebar::tree_rename::Rename;
use crate::sidebar::workspace_groups::{Hold, held_id, next_hold};

/// Rows laid out above and below the visible ones, so a scroll does not show them late.
const OVERDRAW: f32 = 200.;

/// shortcut: ages use UTC for dates a week old or more; pass the local offset once the app has a
/// helper for it.
pub(super) const OFFSET_MINUTES: i32 = 0;

pub(super) use crate::app::clock::now_ms;

/// What the rows read from the dashboard besides the lines, taken once per frame.
pub(super) struct Snapshot {
    pub(super) current: Option<String>,
    pub(super) blurred: Vec<String>,
    pub(super) seen: Vec<(String, TaskState)>,
    /// The tooltip of each task's title, already masked for a blurred project.
    pub(super) tips: BTreeMap<String, String>,
    /// The short word on the right of each attention row.
    pub(super) labels: BTreeMap<String, String>,
    /// What each terminal's agent does: task id, then terminal name.
    pub(super) terminal_activity: BTreeMap<String, BTreeMap<String, String>>,
    pub(super) now_ms: i64,
    /// The tree has focus, so the row with the tab stop shows the focus ring.
    pub(super) focused: bool,
    pub(super) stop: Option<String>,
}

pub struct WorkspaceTree {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) focus: FocusHandle,
    /// The lines of the tree: only the ones on screen are laid out.
    list: ListState,
    /// What the rows read this frame besides the lines.
    pub(super) shown: Option<Snapshot>,
    /// The row with the tree's tab stop, by entry key.
    pub(super) active: Option<String>,
    hold: Hold,
    pub(super) rename: Option<Rename>,
    /// The word typed for type-ahead, and when its last letter came.
    typed: (String, i64),
    /// The tree as last drawn, for keys and menus.
    pub(super) model: Option<TreeModel>,
    /// The model was built after the dashboard's last change.
    model_fresh: bool,
    _observe: Subscription,
    /// Draws the tree again when the next age label changes; only while the window has focus.
    clock: Option<gpui::Task<()>>,
}

impl WorkspaceTree {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&dashboard, |tree, _, cx| {
            tree.model_fresh = false;
            cx.notify();
        });
        WorkspaceTree {
            dashboard,
            focus: cx.focus_handle(),
            list: ListState::new(0, ListAlignment::Top, px(OVERDRAW)),
            shown: None,
            active: None,
            hold: Hold::default(),
            rename: None,
            typed: (String::new(), 0),
            model: None,
            model_fresh: false,
            _observe: observe,
            clock: None,
        }
    }

    /// The list of the tree, for `--gate2`.
    pub fn list_state(&self) -> &ListState {
        &self.list
    }

    fn entry_keys(&self) -> Vec<String> {
        self.model.as_ref().map_or_else(Vec::new, |model| {
            model
                .entries
                .iter()
                .map(|entry| entry.key.clone())
                .collect()
        })
    }

    /// The row with the tab stop: the one last focused, else the open task, else the first row.
    fn stop(&self, current: Option<&str>) -> Option<String> {
        let keys = self.entry_keys();
        let preferred = current.map(|id| format!("task:{id}"));
        [self.active.clone(), preferred]
            .into_iter()
            .flatten()
            .find(|key| keys.contains(key))
            .or_else(|| keys.first().cloned())
    }

    fn current_stop(&self, cx: &App) -> Option<String> {
        self.stop(self.dashboard.read(cx).open_task.as_deref())
    }

    fn line_index(&self, key: &str) -> Option<usize> {
        let model = self.model.as_ref()?;
        let groups = &model.groups.groups;
        model
            .lines
            .iter()
            .position(|line| line.key(groups).as_deref() == Some(key))
    }

    /// Where a menu opened by the keyboard goes: under the row.
    pub(super) fn row_corner(&self, key: &str) -> Point<Pixels> {
        self.line_index(key)
            .and_then(|index| self.list.bounds_for_item(index))
            .map_or_else(
                || self.list.viewport_bounds().origin,
                |bounds| bounds.bottom_left(),
            )
    }

    /// Puts the roving focus on a task's row, for the row menu shot.
    pub fn focus_task(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_key(format!("task:{id}"), window, cx);
    }

    pub(super) fn focus_key(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.line_index(&key) {
            self.list.scroll_to_reveal_item(index);
        }
        self.active = Some(key);
        window.focus(&self.focus, cx);
        cx.notify();
    }

    fn move_by(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let keys = self.entry_keys();
        let Some(stop) = self.current_stop(cx) else {
            return;
        };
        let index = keys.iter().position(|each| *each == stop).unwrap_or(0);
        if let Some(next) = move_target(key, index, keys.len(), false) {
            self.focus_key(keys[next].clone(), window, cx);
        }
    }

    fn select_next(&mut self, _: &SelectNext, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by("ArrowDown", window, cx);
    }

    fn select_previous(&mut self, _: &SelectPrevious, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by("ArrowUp", window, cx);
    }

    fn select_first(&mut self, _: &SelectFirst, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by("Home", window, cx);
    }

    fn select_last(&mut self, _: &SelectLast, window: &mut Window, cx: &mut Context<Self>) {
        self.move_by("End", window, cx);
    }

    pub(super) fn run_entry(&mut self, action: EntryAction, cx: &mut Context<Self>) {
        self.dashboard.update(cx, |dashboard, cx| match action {
            EntryAction::OpenTask(id) => dashboard.open(&id, cx),
            EntryAction::SetFold { key, open } => dashboard.set_fold(&key, open, cx),
        });
    }

    /// Enter: a task opens, a group or "N finished" folds, a terminal row opens its task.
    pub(super) fn activate(&mut self, key: &str, cx: &mut Context<Self>) {
        if let Some(id) = key.strip_prefix("task:") {
            self.run_entry(EntryAction::OpenTask(id.to_owned()), cx);
        } else if let Some(rest) = key.strip_prefix("agent:") {
            let task_id = rest.split_once(':').map_or(rest, |(id, _)| id).to_owned();
            let shell = self.model.as_ref().and_then(|model| {
                let row = model
                    .rows_of(&task_id)
                    .iter()
                    .find(|row| format!("agent:{task_id}:{}", row.key) == key)?;
                row.shell.clone()
            });
            self.dashboard.update(cx, |dashboard, cx| {
                dashboard.open_shell(&task_id, shell, cx)
            });
        } else {
            let toggle = self.model.as_ref().and_then(|model| {
                let entry = model.entries.iter().find(|entry| entry.key == key)?;
                entry.toggle.clone()
            });
            if let Some(toggle) = toggle {
                self.run_entry(toggle, cx);
            }
        }
    }

    /// The Answer button: the workspace on the terminal whose agent asked, else the first agent
    /// terminal.
    pub(super) fn answer(&mut self, task_id: &str, cx: &mut Context<Self>) {
        let shell = self.model.as_ref().and_then(|model| {
            let rows = model.rows_of(task_id);
            let asking = rows
                .iter()
                .find(|row| row.state == Some(TaskState::NeedsInput))?;
            asking.shell.clone()
        });
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.open_shell(task_id, shell, cx));
    }

    fn open_selected(&mut self, _: &OpenSelected, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(stop) = self.current_stop(cx) {
            self.activate(&stop, cx);
        }
    }

    fn fold_key(&mut self, event_key: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stop) = self.current_stop(cx) else {
            return;
        };
        let entries = self.model.as_ref().map_or(&[][..], |model| &model.entries);
        match tree_key(event_key, &stop, entries) {
            TreeKey::Toggle(action) => self.run_entry(action, cx),
            TreeKey::Focus(key) => self.focus_key(key, window, cx),
            TreeKey::Handled | TreeKey::Ignored => {}
        }
    }

    fn collapse(&mut self, _: &CollapseRow, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_key("ArrowLeft", window, cx);
    }

    fn expand(&mut self, _: &ExpandRow, window: &mut Window, cx: &mut Context<Self>) {
        self.fold_key("ArrowRight", window, cx);
    }

    fn rename_selected(&mut self, _: &RenameRow, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.current_stop(cx).and_then(|key| task_of(&key)) {
            self.start_rename(&id, window, cx);
        }
    }

    fn menu_selected(&mut self, _: &OpenRowMenu, window: &mut Window, cx: &mut Context<Self>) {
        let Some(stop) = self.current_stop(cx) else {
            return;
        };
        let corner = self.row_corner(&stop);
        if let Some(id) = task_of(&stop) {
            self.open_row_menu(&id, corner, window, cx);
        } else if let Some(repo) = stop.strip_prefix("repo:") {
            self.open_project_menu(repo, corner, window, cx);
        }
    }

    fn remove_selected(&mut self, _: &RemoveRow, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.current_stop(cx).and_then(|key| task_of(&key)) {
            self.remove_workspace(&id, window, cx);
        }
    }

    /// Letters jump to the next row whose title starts with what was typed.
    fn type_ahead(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let stroke = &event.keystroke;
        let letter = stroke.key.chars().count() == 1 && stroke.key != " ";
        if self.rename.is_some() || !letter || stroke.modifiers.modified() {
            return;
        }
        let now = now_ms();
        let word = typed_word(&self.typed.0, self.typed.1, now, &stroke.key);
        self.typed = (word.clone(), now);
        let Some(model) = self.model.as_ref() else {
            return;
        };
        let labels: Vec<String> = model.entries.iter().map(|e| e.label.clone()).collect();
        let stop = self.current_stop(cx);
        let from = stop
            .and_then(|stop| model.entries.iter().position(|entry| entry.key == stop))
            .unwrap_or(0);
        if let Some(found) = type_ahead_target(&labels, &word, from) {
            let key = model.entries[found].key.clone();
            self.focus_key(key, window, cx);
            cx.stop_propagation();
        }
    }

    /// The tree of the dashboard as it is now. It is built again only after the dashboard
    /// changed: the beat of a running glyph and the age clock draw the same tree.
    fn build_model(&mut self, cx: &App) {
        if self.model_fresh && self.model.is_some() {
            return;
        }
        let dashboard = self.dashboard.read(cx);
        let data = &dashboard.data;
        let view = dashboard.view();
        let current = dashboard.open_task.as_deref();
        self.hold = next_hold(&self.hold, current, &data.tasks);
        let model = TreeModel::build(&TreeInput {
            repos: &data.repos,
            tasks: &data.tasks,
            filter: &dashboard.filter,
            current_id: current,
            held_id: held_id(&self.hold),
            pinned: &view.pinned,
            folds: &view.folds,
            terminals: &data.terminals,
        });
        dashboard.show_terminals(TerminalReader::Sidebar, model.reads_terminals.clone());
        // A fold or a new task changes the lines: the list keeps the row at its top.
        let top = self.list.logical_scroll_top();
        self.list.reset(model.lines.len());
        self.list.scroll_to(top);
        self.model = Some(model);
        self.model_fresh = true;
        self.clock = None;
    }

    /// Draws the tree again when the soonest time label changes (`follow_labels`).
    fn follow_clock(&mut self, now: i64, cx: &mut Context<Self>) {
        let wait = self
            .model
            .as_ref()
            .and_then(|model| model.next_label_change_ms(now));
        follow_labels(&mut self.clock, wait, cx, |tree, cx| {
            tree.clock = None;
            cx.notify();
        });
    }

    fn snapshot(&mut self, window: &Window, cx: &App) -> Snapshot {
        self.build_model(cx);
        let dashboard = self.dashboard.read(cx);
        let data = &dashboard.data;
        let view = dashboard.view();
        let current = dashboard.open_task.clone();
        let mut snapshot = Snapshot {
            current,
            blurred: view.blurred.clone(),
            seen: view.seen.clone(),
            tips: BTreeMap::new(),
            labels: BTreeMap::new(),
            terminal_activity: BTreeMap::new(),
            now_ms: now_ms(),
            focused: self.focus.contains_focused(window, cx),
            stop: None,
        };
        let lines = self.model.as_ref().map_or(&[][..], |model| &model.lines);
        for line in lines {
            let Line::Task {
                task, pinned_row, ..
            } = line
            else {
                continue;
            };
            snapshot.add_task(data, task, *pinned_row, &view.blurred);
        }
        snapshot
    }
}

/// The task id of a task row's key.
pub(super) fn task_of(key: &str) -> Option<String> {
    key.strip_prefix("task:").map(str::to_owned)
}

impl Focusable for WorkspaceTree {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for WorkspaceTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut snapshot = self.snapshot(window, cx);
        snapshot.stop = self.stop(snapshot.current.as_deref());
        self.follow_clock(snapshot.now_ms, cx);
        self.shown = Some(snapshot);
        let rows = cx.processor(|tree, index: usize, _, cx| tree.render_line(index, cx));
        // Bindings of lists stay off while the rename field takes keys.
        let context = if self.rename.is_some() {
            "NatamuxList WorkspaceTree editing"
        } else {
            "NatamuxList WorkspaceTree not_editing"
        };
        v_flex()
            .id("workspace-tree")
            .key_context(context)
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::select_next))
            .on_action(cx.listener(Self::select_previous))
            .on_action(cx.listener(Self::select_first))
            .on_action(cx.listener(Self::select_last))
            .on_action(cx.listener(Self::open_selected))
            .on_action(cx.listener(Self::collapse))
            .on_action(cx.listener(Self::expand))
            .on_action(cx.listener(Self::rename_selected))
            .on_action(cx.listener(Self::menu_selected))
            .on_action(cx.listener(Self::remove_selected))
            .on_action(cx.listener(|tree, _: &Back, window, cx| {
                if tree.rename.is_some() {
                    tree.finish_rename(false, true, window, cx);
                } else {
                    cx.propagate();
                }
            }))
            .on_key_down(cx.listener(Self::type_ahead))
            .size_full()
            .min_h_0()
            // Here, not on the list: a list keeps only its top and bottom padding.
            .px_2()
            .child(list(self.list.clone(), rows).size_full().pb_3())
    }
}
