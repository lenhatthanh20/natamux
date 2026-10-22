//! The tree of workspaces in the sidebar (docs/ui-spec.md 4.9, `WorkspaceList.tsx`). The whole
//! tree is one tab stop: arrows, Home and End move, Left and Right fold and unfold, Enter opens,
//! F2 renames, Shift+F10 opens the row menu, Delete asks to remove the workspace, and letters jump
//! to a row by title. What it shows comes from `tree_model`; `workspace_row` and `project_header`
//! draw the lines.

use std::collections::BTreeMap;
use std::time::Duration;

use editor::{Editor, EditorEvent};
use gpui::{
    Entity, FocusHandle, Focusable, KeyDownEvent, Pixels, Point, ScrollHandle, Subscription,
};
use natamux_proto::agents::activity::latest_line;
use natamux_proto::projects::repo::Repo;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use crate::app::dashboard::{Dashboard, TerminalReader};
use crate::app::shortcuts::{
    Back, CollapseRow, ExpandRow, OpenRowMenu, OpenSelected, RemoveRow, RenameRow, SelectFirst,
    SelectLast, SelectNext, SelectPrevious,
};
use crate::sidebar::sidebar_text::{card_line, renamed_title, type_ahead_target, typed_word};
use crate::sidebar::sidebar_tree::{RowTier, row_tier, subtask_count};
use crate::sidebar::tree_entries::{EntryAction, TreeKey, move_target, tree_key};
use crate::sidebar::tree_model::{Line, TreeInput, TreeModel};
use crate::sidebar::workspace_groups::{Hold, held_id, next_hold};

/// The ages on the rows move on: the tree draws again this often.
const CLOCK: Duration = Duration::from_secs(30);

/// shortcut: ages use UTC for dates a week old or more; pass the local offset once the app has a
/// helper for it.
pub(super) const OFFSET_MINUTES: i32 = 0;

pub(super) use crate::app::clock::now_ms;

/// The inline rename of a row: a one-line editor in place of the title.
pub(super) struct Rename {
    pub(super) task_id: String,
    pub(super) editor: Entity<Editor>,
    _blur: Subscription,
}

/// What the rows read from the dashboard besides the lines, taken once per frame.
pub(super) struct Snapshot {
    pub(super) current: Option<String>,
    pub(super) blurred: Vec<String>,
    pub(super) repos: Vec<Repo>,
    pub(super) seen: Vec<(String, TaskState)>,
    /// The question or failure reason of each attention card.
    pub(super) cards: BTreeMap<String, String>,
    /// What the agent of each task does now.
    pub(super) activity: BTreeMap<String, String>,
    /// The same per terminal: task id, then terminal name.
    pub(super) terminal_activity: BTreeMap<String, BTreeMap<String, String>>,
    /// "2/5" subtasks done, for tasks that have some.
    pub(super) subtasks: BTreeMap<String, String>,
    pub(super) now_ms: i64,
    /// The tree has focus, so the row with the tab stop shows the focus ring.
    pub(super) focused: bool,
    pub(super) stop: Option<String>,
}

pub struct WorkspaceTree {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) focus: FocusHandle,
    pub(super) scroll: ScrollHandle,
    /// The row with the tree's tab stop, by entry key.
    active: Option<String>,
    hold: Hold,
    pub(super) rename: Option<Rename>,
    /// The word typed for type-ahead, and when its last letter came.
    typed: (String, i64),
    /// The tree as last drawn, for keys and menus.
    pub(super) model: Option<TreeModel>,
    _observe: Subscription,
    _clock: gpui::Task<()>,
}

impl WorkspaceTree {
    pub fn new(dashboard: Entity<Dashboard>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&dashboard, |_, _, cx| cx.notify());
        let clock = cx.spawn(async move |tree, cx| {
            loop {
                cx.background_executor().timer(CLOCK).await;
                if tree.update(cx, |_, cx| cx.notify()).is_err() {
                    break;
                }
            }
        });
        WorkspaceTree {
            dashboard,
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            active: None,
            hold: Hold::default(),
            rename: None,
            typed: (String::new(), 0),
            model: None,
            _observe: observe,
            _clock: clock,
        }
    }

    /// The scroll of the tree, for `--gate2`.
    pub fn scroll_handle(&self) -> &ScrollHandle {
        &self.scroll
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
            .and_then(|index| self.scroll.bounds_for_item(index))
            .map_or_else(
                || self.scroll.bounds().origin,
                |bounds| bounds.bottom_left(),
            )
    }

    /// Puts the roving focus on a task's row, for the row menu shot.
    pub fn focus_task(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_key(format!("task:{id}"), window, cx);
    }

    pub(super) fn focus_key(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(index) = self.line_index(&key) {
            self.scroll.scroll_to_item(index);
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

    pub(super) fn start_rename(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task) = self.dashboard.read(cx).data.task(task_id).cloned() else {
            return;
        };
        let editor = cx.new(|cx| {
            let mut editor = Editor::single_line(window, cx);
            editor.set_text(task.title.clone(), window, cx);
            editor.select_all(&editor::actions::SelectAll, window, cx);
            editor
        });
        // Leaving the field keeps the name, as Enter does.
        let blur = cx.subscribe_in(
            &editor,
            window,
            |tree, _, event: &EditorEvent, window, cx| {
                if matches!(event, EditorEvent::Blurred) {
                    tree.finish_rename(true, false, window, cx);
                }
            },
        );
        window.focus(&editor.focus_handle(cx), cx);
        self.active = Some(format!("task:{task_id}"));
        self.rename = Some(Rename {
            task_id: task_id.to_owned(),
            editor,
            _blur: blur,
        });
        cx.notify();
    }

    /// `keep`: Enter or a blur saves, Esc drops. `refocus`: focus goes back to the row.
    pub(super) fn finish_rename(
        &mut self,
        keep: bool,
        refocus: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(rename) = self.rename.take() else {
            return;
        };
        let draft = rename.editor.read(cx).text(cx);
        let title = self
            .dashboard
            .read(cx)
            .data
            .task(&rename.task_id)
            .map(|task| task.title.clone())
            .unwrap_or_default();
        if keep && let Some(title) = renamed_title(&draft, &title) {
            self.dashboard
                .update(cx, |dashboard, cx| {
                    dashboard.rename_task(&rename.task_id, &title, cx)
                })
                .detach();
        }
        if refocus {
            window.focus(&self.focus, cx);
        }
        cx.notify();
    }

    fn snapshot(&mut self, window: &Window, cx: &App) -> (TreeModel, Snapshot) {
        let dashboard = self.dashboard.read(cx);
        let data = &dashboard.data;
        let view = dashboard.view();
        let current = dashboard.open_task.clone();
        self.hold = next_hold(&self.hold, current.as_deref(), &data.tasks);
        let model = TreeModel::build(&TreeInput {
            repos: &data.repos,
            tasks: &data.tasks,
            filter: &dashboard.filter,
            current_id: current.as_deref(),
            held_id: held_id(&self.hold),
            pinned: &view.pinned,
            folds: &view.folds,
            terminals: &data.terminals,
        });
        dashboard.show_terminals(TerminalReader::Sidebar, model.reads_terminals.clone());
        let mut snapshot = Snapshot {
            current,
            blurred: view.blurred.clone(),
            repos: data.repos.clone(),
            seen: view.seen.clone(),
            cards: BTreeMap::new(),
            activity: BTreeMap::new(),
            terminal_activity: BTreeMap::new(),
            subtasks: BTreeMap::new(),
            now_ms: now_ms(),
            focused: self.focus.contains_focused(window, cx),
            stop: None,
        };
        for line in &model.lines {
            let Line::Task { task, .. } = line else {
                continue;
            };
            let id = &task.id;
            if row_tier(task.state) == RowTier::Attention {
                let events = data.events.get(id).map(|(_, events)| events.as_slice());
                if let Some(text) = card_line(task, events) {
                    snapshot.cards.insert(id.clone(), text);
                }
            }
            if let Some(activity) = data.activity.get(id) {
                if let Some(text) = latest_line(activity) {
                    snapshot.activity.insert(id.clone(), text.to_owned());
                }
                let per_terminal = activity
                    .iter()
                    .map(|(terminal, line)| (terminal.clone(), line.text.clone()))
                    .collect();
                snapshot.terminal_activity.insert(id.clone(), per_terminal);
            }
            if let Some(count) = subtask_count(id, &data.tasks) {
                let text = format!("{}/{}", count.done, count.total);
                snapshot.subtasks.insert(id.clone(), text);
            }
        }
        (model, snapshot)
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
        let (model, mut snapshot) = self.snapshot(window, cx);
        self.model = Some(model);
        snapshot.stop = self.stop(snapshot.current.as_deref());
        let lines = self.render_lines(&snapshot, cx);
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
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .px_2()
            .pb_3()
            .children(lines)
    }
}
