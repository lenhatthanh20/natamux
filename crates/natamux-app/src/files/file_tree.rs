//! The Files tab of the details panel (`FileTree.tsx`): the worktree as a tree, read one folder at
//! a time. A click or Enter opens a file in the editor tab or folds a folder; a row can be
//! dragged onto a terminal of the same task (see `terminals/pane_drop.rs`).

use std::collections::BTreeSet;

use gpui::{ClickEvent, Entity, FocusHandle, KeyDownEvent, ScrollHandle, Subscription};
use natamux_proto::workspaces::files::EntryKind;
use ui::{Tooltip, prelude::*};

use super::draft_store::DraftStore;
use super::explorer_tree::{
    ExplorerRow, change_marks, explorer_rows, first_child_key, folders_above, parent_key,
};
use super::file_reads::{FileReads, keep_open_folders, open_folders};
use super::file_tree_rows::{RowLook, tree_row};
use crate::app::clock::now_ms;
use crate::app::dashboard::Dashboard;
use crate::components::skeleton::SkeletonList;
use crate::sidebar::sidebar_text::{type_ahead_target, typed_word};

pub struct FileTree {
    pub(super) dashboard: Entity<Dashboard>,
    pub(super) task_id: String,
    pub(super) expanded: BTreeSet<String>,
    pub(super) reads: FileReads,
    /// The row the arrow keys are on.
    cursor: Option<String>,
    /// The file of the editor tab as last seen, so a new one opens its folders once.
    editor_file: Option<String>,
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// The word typed for type-ahead, and when its last letter came.
    typed: (String, i64),
    drafts: Entity<DraftStore>,
    _subscriptions: Vec<Subscription>,
}

impl FileTree {
    pub fn new(
        dashboard: Entity<Dashboard>,
        task_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let drafts = DraftStore::global(cx);
        let subscriptions = vec![
            cx.observe(&drafts, |_, _, cx| cx.notify()),
            cx.observe(&dashboard, |tree, _, cx| tree.sync_editor_file(cx)),
        ];
        let mut tree = FileTree {
            expanded: open_folders(&task_id, cx),
            dashboard,
            task_id,
            reads: FileReads::default(),
            cursor: None,
            editor_file: None,
            focus: cx.focus_handle().tab_stop(true),
            scroll: ScrollHandle::new(),
            typed: (String::new(), 0),
            drafts,
            _subscriptions: subscriptions,
        };
        tree.sync_editor_file(cx);
        tree.read_new_folders(cx);
        tree.read_changes(false, cx);
        tree
    }

    pub fn task_id(&self) -> &str {
        &self.task_id
    }

    /// `workspace.dirty`: the shown folders and the changes again.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.read_all(false, cx);
    }

    /// A file opened from elsewhere shows with its folders open.
    fn sync_editor_file(&mut self, cx: &mut Context<Self>) {
        let now: Option<String> = self
            .dashboard
            .read(cx)
            .files
            .editor_file(&self.task_id)
            .map(String::from);
        if now == self.editor_file {
            return;
        }
        self.editor_file = now.clone();
        if let Some(path) = now {
            let above = folders_above(&path);
            if above.iter().any(|folder| !self.expanded.contains(folder)) {
                self.expanded.extend(above);
                keep_open_folders(&self.task_id, &self.expanded, cx);
                self.read_new_folders(cx);
            }
            self.cursor = Some(path);
        }
        cx.notify();
    }

    fn rows(&self) -> Vec<ExplorerRow> {
        explorer_rows(&self.reads.folders, &self.expanded)
    }

    fn toggle(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(key) {
            self.expanded.insert(key.to_owned());
        }
        keep_open_folders(&self.task_id, &self.expanded, cx);
        self.read_new_folders(cx);
        cx.notify();
    }

    /// Enter, Space and a click: a file opens in the editor tab, a folder folds or opens.
    fn activate(&mut self, row: &ExplorerRow, cx: &mut Context<Self>) {
        self.cursor = Some(row.key.clone());
        if row.kind == EntryKind::File {
            let (task, path) = (self.task_id.clone(), row.key.clone());
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.open_file(&task, &path, cx));
        } else {
            self.toggle(&row.key, cx);
        }
        cx.notify();
    }

    fn move_to(&mut self, rows: &[ExplorerRow], index: usize, cx: &mut Context<Self>) {
        if let Some(row) = rows.get(index) {
            self.cursor = Some(row.key.clone());
            self.scroll.scroll_to_item(index);
            cx.notify();
        }
    }

    fn move_to_key(&mut self, rows: &[ExplorerRow], key: Option<String>, cx: &mut Context<Self>) {
        let index = key.and_then(|key| rows.iter().position(|row| row.key == key));
        if let Some(index) = index {
            self.move_to(rows, index, cx);
        }
    }

    fn click(&mut self, key: &str, window: &mut Window, cx: &mut Context<Self>) {
        window.focus(&self.focus, cx);
        if let Some(row) = self.rows().into_iter().find(|row| row.key == key) {
            self.activate(&row, cx);
        }
    }

    /// Up, Down, Home, End, Right, Left, Enter, Space and letters (`useRovingFocus`).
    fn on_key(&mut self, event: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let rows = self.rows();
        if rows.is_empty() || event.keystroke.modifiers.modified() {
            return;
        }
        let at = self
            .cursor
            .as_ref()
            .or(self.editor_file.as_ref())
            .and_then(|key| rows.iter().position(|row| &row.key == key))
            .unwrap_or(0);
        let last = rows.len() - 1;
        let row = &rows[at];
        match event.keystroke.key.as_str() {
            "down" => self.move_to(&rows, (at + 1).min(last), cx),
            "up" => self.move_to(&rows, at.saturating_sub(1), cx),
            "home" => self.move_to(&rows, 0, cx),
            "end" => self.move_to(&rows, last, cx),
            "right" if row.expandable && !row.expanded => self.toggle(&row.key.clone(), cx),
            "right" if row.expandable => {
                self.move_to_key(&rows, first_child_key(&rows, &row.key), cx)
            }
            "left" if row.expanded => self.toggle(&row.key.clone(), cx),
            "left" => self.move_to_key(&rows, parent_key(&rows, &row.key), cx),
            "enter" | "space" => self.activate(&row.clone(), cx),
            key => return self.type_ahead(key, &rows, at, cx),
        }
        cx.stop_propagation();
    }

    fn type_ahead(&mut self, key: &str, rows: &[ExplorerRow], at: usize, cx: &mut Context<Self>) {
        let mut letters = key.chars();
        let (Some(letter), None) = (letters.next(), letters.next()) else {
            return;
        };
        if !letter.is_alphanumeric() {
            return;
        }
        let now = now_ms();
        let word = typed_word(&self.typed.0, self.typed.1, now, key);
        self.typed = (word.clone(), now);
        let labels: Vec<String> = rows.iter().map(|row| row.name.clone()).collect();
        if let Some(found) = type_ahead_target(&labels, &word, at) {
            self.move_to(rows, found, cx);
            cx.stop_propagation();
        }
    }

    fn header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        h_flex()
            .h(px(32.))
            .flex_none()
            .pl_3()
            .pr_1()
            .gap_2()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                div().flex_1().min_w_0().child(
                    Label::new("Drag a file onto a terminal to type its path.")
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .truncate(),
                ),
            )
            .child(
                IconButton::new("files-refresh", IconName::RotateCw)
                    .icon_size(IconSize::Small)
                    .icon_color(Color::Muted)
                    .tooltip(Tooltip::text("Refresh"))
                    .on_click(cx.listener(|tree, _: &ClickEvent, _, cx| tree.read_all(true, cx))),
            )
    }

    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        if let Some(message) = &self.reads.root_error {
            return div()
                .p_4()
                .child(Label::new(message.clone()).color(Color::Error))
                .into_any_element();
        }
        if !self.reads.folders.contains_key("") {
            return div()
                .p_1()
                .child(SkeletonList::new(8, 28.))
                .into_any_element();
        }
        let rows = self.rows();
        if rows.is_empty() {
            return div()
                .p_3()
                .child(Label::new("No files.").color(Color::Muted))
                .into_any_element();
        }
        let marks = change_marks(&self.reads.changes);
        let focused = self.focus.contains_focused(window, cx);
        let editor_file = self.editor_file.clone();
        let items: Vec<AnyElement> = rows
            .iter()
            .enumerate()
            .map(|(index, row)| {
                let is_file = row.kind == EntryKind::File;
                let look = RowLook {
                    status: marks.files.get(&row.key).copied(),
                    changed_below: marks.folders.contains(&row.key),
                    unsaved: self.drafts.read(cx).file(&self.task_id, &row.key).is_some(),
                    selected: is_file && editor_file.as_deref() == Some(row.key.as_str()),
                    tree_focused: focused,
                    at_cursor: focused && self.cursor.as_deref() == Some(row.key.as_str()),
                };
                let key = row.key.clone();
                tree_row(index, row, &self.task_id, &look, cx)
                    .on_click(cx.listener(move |tree, _: &ClickEvent, window, cx| {
                        tree.click(&key, window, cx)
                    }))
                    .into_any_element()
            })
            .collect();
        v_flex()
            .id("file-tree")
            .track_focus(&self.focus)
            .track_scroll(&self.scroll)
            .on_key_down(cx.listener(Self::on_key))
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1()
            .children(items)
            .into_any_element()
    }
}

impl Render for FileTree {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let body = self.body(window, cx);
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.header(cx))
            .child(body)
    }
}
