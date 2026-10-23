//! The file panel of the Changes tab (`ChangedFiles.tsx`, `ChangedRow.tsx`): the header with the
//! counts and its buttons, the files as a list or a tree, their row menu (Rollback…), the keys
//! of the tree. The picker of the narrow layout and the rows' parts are in `changes_rows.rs`.

use std::collections::BTreeSet;

use gpui::{ClickEvent, KeyDownEvent, MouseButton, MouseDownEvent, Point};
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::{ContextMenu, ContextMenuEntry, Disclosure, Tooltip, prelude::*};

use super::ChangesView;
use super::rows::{counts_label, file_row, zed_button};
use crate::components::colors::change_color;
use crate::components::menu::MenuLayer;
use crate::components::resize_rules::Limits;
use crate::git::change_rules::{count_label, status_tone};
use crate::git::change_tree::{
    FileViewMode, TreeNode, TreeRow, all_folder_keys, ancestor_keys, build_change_tree,
    entries_under, flatten_tree, list_rows, total_counts,
};
use crate::git::diff_rules::COUNTS_FROM;
use crate::git::rollback_dialog;
use crate::git::rollback_rules::rollback_offer;
use crate::terminals::pane_header::icon_button;

/// The file panel's width (`useStoredWidth('natamux.files-width', 260, 200, 420)`).
pub const FILES_LIMITS: Limits = Limits {
    min: 200.,
    max: 420.,
    default: 260.,
    collapse_below: None,
};

/// One row as drawn: what it shows and what a click does.
struct RowFacts {
    key: String,
    depth: usize,
    node: TreeNode,
    expanded: bool,
}

impl ChangesView {
    fn rows(&self, cx: &App) -> Vec<RowFacts> {
        let Some(list) = self.list.as_deref() else {
            return Vec::new();
        };
        let facts = |row: TreeRow| RowFacts {
            key: row.key,
            depth: row.level - 1,
            node: row.node.into_owned(),
            expanded: row.expanded,
        };
        match self.mode(cx) {
            FileViewMode::List => list_rows(list).into_iter().map(facts).collect(),
            FileViewMode::Tree => {
                let nodes = build_change_tree(list, true);
                let folded = self.folded_now();
                flatten_tree(&nodes, &folded)
                    .into_iter()
                    .map(facts)
                    .collect()
            }
        }
    }

    /// The folded folders of the task on screen; the picked file's folders are always open.
    fn folded_now(&self) -> BTreeSet<String> {
        let mut folded = self
            .task_id
            .as_ref()
            .and_then(|id| self.folded.get(id))
            .cloned()
            .unwrap_or_default();
        if let (Some(list), Some(path)) = (self.list.as_deref(), self.picked.as_deref()) {
            for key in ancestor_keys(&build_change_tree(list, true), path) {
                folded.remove(&key);
            }
        }
        folded
    }

    fn set_folded(&mut self, folded: BTreeSet<String>, cx: &mut Context<Self>) {
        if let Some(id) = self.task_id.clone() {
            self.folded.insert(id, folded);
        }
        cx.notify();
    }

    fn toggle_folder(&mut self, key: &str, cx: &mut Context<Self>) {
        let mut folded = self.folded_now();
        if !folded.remove(key) {
            folded.insert(key.to_owned());
        }
        self.set_folded(folded, cx);
    }

    fn fold_all(&mut self, fold: bool, cx: &mut Context<Self>) {
        let keys = self
            .list
            .as_deref()
            .map(|list| all_folder_keys(&build_change_tree(list, true)))
            .unwrap_or_default();
        let folded = if fold {
            keys.into_iter().collect()
        } else {
            BTreeSet::new()
        };
        self.set_folded(folded, cx);
    }

    /// A click or Enter on a row: a file shows, a folder folds or opens.
    fn activate(&mut self, row: &RowFacts, cx: &mut Context<Self>) {
        self.cursor = Some(row.key.clone());
        match &row.node {
            TreeNode::File(file) => self.pick(&file.entry.path.clone(), cx),
            TreeNode::Folder(_) => self.toggle_folder(&row.key, cx),
        }
    }

    pub(super) fn open_rollback(
        &mut self,
        files: Vec<ChangeEntry>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if files.is_empty() {
            return;
        }
        rollback_dialog::open(&self.dashboard, self.workspace.clone(), files, window, cx);
    }

    fn open_row_menu(
        &mut self,
        node: TreeNode,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let offer = rollback_offer(entries_under(&node));
        let this = cx.weak_entity();
        let focus = self.files_focus.clone();
        let name = node.name().to_owned();
        let menu = ContextMenu::build(window, cx, move |menu, _, _| {
            let menu = menu.context(focus).header(format!("Actions for {name}"));
            let entry = ContextMenuEntry::new("Rollback…")
                .custom_icon_path("icons/lucide/undo-2.svg")
                .icon_position(IconPosition::Start)
                .icon_color(Color::Muted)
                .disabled(offer.off_reason.is_some())
                .handler(move |window, cx| {
                    let files = offer.entries.clone();
                    this.update(cx, |view, cx| view.open_rollback(files, window, cx))
                        .ok();
                });
            let menu = menu.item(entry);
            match offer.off_reason {
                Some(reason) => menu.label(reason),
                None => menu,
            }
        });
        MenuLayer::open(menu, position, window, cx);
    }

    /// Arrow keys, Home, End, Enter, Space and type-ahead, as in VS Code; Shift+F10 opens the
    /// row menu. `[` and `]` are not type-ahead: they step files.
    fn on_files_key(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        if rows.is_empty() {
            return;
        }
        let at = self
            .cursor
            .as_ref()
            .and_then(|key| rows.iter().position(|row| &row.key == key))
            .unwrap_or(0);
        let key = event.keystroke.key.as_str();
        let target = match key {
            "down" => Some((at + 1).min(rows.len() - 1)),
            "up" => Some(at.saturating_sub(1)),
            "home" => Some(0),
            "end" => Some(rows.len() - 1),
            _ => None,
        };
        let row = &rows[at];
        match key {
            _ if target.is_some() => {
                if let Some(target) = target {
                    self.activate_or_move(&rows[target], cx);
                }
            }
            "right" if row.expanded => self.move_cursor(rows.get(at + 1), cx),
            "right" | "left" if matches!(row.node, TreeNode::Folder(_)) => {
                if (key == "right") != row.expanded {
                    self.toggle_folder(&row.key.clone(), cx);
                }
            }
            "enter" | "space" => self.activate(row, cx),
            "f10" if event.keystroke.modifiers.shift => {
                self.open_row_menu(row.node.clone(), window.mouse_position(), window, cx)
            }
            _ => return self.type_ahead(key, &rows, at, cx),
        }
        cx.stop_propagation();
    }

    /// Moving onto a file shows it, as a picker does; a folder only takes the cursor.
    fn activate_or_move(&mut self, row: &RowFacts, cx: &mut Context<Self>) {
        match &row.node {
            TreeNode::File(_) => self.activate(row, cx),
            TreeNode::Folder(_) => self.move_cursor(Some(row), cx),
        }
    }

    fn move_cursor(&mut self, row: Option<&RowFacts>, cx: &mut Context<Self>) {
        if let Some(row) = row {
            self.cursor = Some(row.key.clone());
            cx.notify();
        }
    }

    fn type_ahead(&mut self, key: &str, rows: &[RowFacts], at: usize, cx: &mut Context<Self>) {
        let mut letters = key.chars();
        let (Some(letter), None) = (letters.next(), letters.next()) else {
            return;
        };
        if !letter.is_alphanumeric() {
            return;
        }
        let found = (1..=rows.len())
            .map(|step| &rows[(at + step) % rows.len()])
            .find(|row| {
                row.node
                    .name()
                    .to_lowercase()
                    .starts_with(letter.to_ascii_lowercase())
            });
        if let Some(row) = found {
            self.activate_or_move(row, cx);
            cx.stop_propagation();
        }
    }

    pub(super) fn render_files(
        &mut self,
        panel: f32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let rows = self.rows(cx);
        let shown = self.shown_path(cx);
        let focused = self.files_focus.contains_focused(window, cx);
        let counts_shown = panel >= COUNTS_FROM;
        let row_elements: Vec<AnyElement> = rows
            .into_iter()
            .enumerate()
            .map(|(index, row)| self.render_row(index, row, &shown, focused, counts_shown, cx))
            .collect();
        v_flex()
            .flex_1()
            .min_h_0()
            .child(self.render_header(cx))
            .child(
                v_flex()
                    .id("changed-files")
                    .track_focus(&self.files_focus)
                    .on_key_down(cx.listener(Self::on_files_key))
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .p_1()
                    .children(row_elements),
            )
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let list = self.list.as_deref().unwrap_or_default();
        let tree = self.mode(cx) == FileViewMode::Tree;
        let has_folders = list.iter().any(|entry| entry.path.contains('/'));
        let any_open = !all_folder_keys(&build_change_tree(list, true))
            .iter()
            .all(|key| self.folded_now().contains(key));
        let totals = total_counts(list).map(|counts| {
            h_flex()
                .id("changes-totals")
                .gap_1()
                .tooltip(Tooltip::text(format!(
                    "{} lines added, {} removed",
                    counts.added, counts.removed
                )))
                .child(counts_label(counts.added, counts.removed, cx))
        });
        let fold = tree.then(|| {
            let (icon, tip) = if any_open {
                (IconName::ChevronDownUp, "Collapse all folders")
            } else {
                (IconName::ChevronUpDown, "Expand all folders")
            };
            zed_button("changes-fold-all", icon)
                .disabled(!has_folders)
                .tooltip(Tooltip::text(tip))
                .on_click(
                    cx.listener(move |view, _: &ClickEvent, _, cx| view.fold_all(any_open, cx)),
                )
        });
        let all = list.to_vec();
        h_flex()
            .h(px(32.))
            .flex_none()
            .px_2()
            .gap_1()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(count_label(list.len()))
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .children(totals)
            .child(div().flex_1())
            .children(fold)
            .child(
                icon_button("changes-view-mode", "list-tree")
                    .toggle_state(tree)
                    .tooltip(Tooltip::text("View as tree"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| {
                        let next = if tree {
                            FileViewMode::List
                        } else {
                            FileViewMode::Tree
                        };
                        view.dashboard
                            .update(cx, |dashboard, cx| dashboard.set_changes_view(next, cx));
                    })),
            )
            .child(
                icon_button("changes-rollback", "undo-2")
                    .disabled(list.is_empty())
                    .tooltip(Tooltip::text("Rollback… (Mod+Alt+Z)"))
                    .on_click(cx.listener(move |view, _: &ClickEvent, window, cx| {
                        view.open_rollback(all.clone(), window, cx)
                    })),
            )
            .child(
                icon_button("changes-refresh", "rotate-ccw")
                    .tooltip(Tooltip::text("Refresh (Shift+R)"))
                    .on_click(cx.listener(|view, _: &ClickEvent, _, cx| view.refresh(cx))),
            )
    }

    fn render_row(
        &self,
        index: usize,
        row: RowFacts,
        shown: &Option<String>,
        focused: bool,
        counts_shown: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let picked =
            matches!(&row.node, TreeNode::File(file) if Some(&file.entry.path) == shown.as_ref());
        let at_cursor = focused && self.cursor.as_deref() == Some(row.key.as_str());
        let indent = if self.mode(cx) == FileViewMode::Tree {
            12. * row.depth as f32
        } else {
            0.
        };
        let content = match &row.node {
            TreeNode::File(file) => file_row(&file.entry, self.mode(cx), counts_shown, cx),
            TreeNode::Folder(folder) => {
                let counts = (!row.expanded)
                    .then(|| folder.added.zip(folder.removed))
                    .flatten()
                    .filter(|_| counts_shown)
                    .map(|(added, removed)| counts_label(added, removed, cx));
                h_flex()
                    .gap_1()
                    .min_w_0()
                    .child(Disclosure::new(("folder", index), row.expanded))
                    .child(
                        Label::new(folder.name.clone())
                            .size(LabelSize::Small)
                            .truncate(),
                    )
                    .child(
                        Label::new(folder.file_count.to_string())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(div().flex_1())
                    .children(counts)
                    .child(
                        div()
                            .size(px(6.))
                            .rounded_full()
                            .bg(change_color(status_tone(folder.status), cx)),
                    )
                    .into_any_element()
            }
        };
        let node = row.node.clone();
        h_flex()
            .id(SharedString::from(format!("changed-row-{}", row.key)))
            .h(px(28.))
            .pl(px(6. + indent))
            .pr_1p5()
            .rounded_sm()
            .cursor_pointer()
            .when(picked, |this| this.bg(colors.element_selected))
            .when(at_cursor, |this| {
                this.border_1().border_color(colors.border_focused)
            })
            .hover(|this| this.bg(colors.element_hover))
            .child(content)
            .on_click(cx.listener(move |view, _: &ClickEvent, _, cx| view.activate(&row, cx)))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |view, event: &MouseDownEvent, window, cx| {
                    view.open_row_menu(node.clone(), event.position, window, cx)
                }),
            )
            .into_any_element()
    }
}
