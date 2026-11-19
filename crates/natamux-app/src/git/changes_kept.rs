//! What the file panel of the Changes tab derives from the list, kept between frames: the tree,
//! the files in the panel's order, the folder keys and the rows. A list of thousands of files
//! would cost a tree build per draw otherwise. `set_list` drops it all when the list changes;
//! the rows are also built again when the mode, the folded folders or the picked file change.

use std::cell::OnceCell;
use std::collections::BTreeSet;
use std::rc::Rc;

use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::ChangeEntry;
use ui::prelude::*;

use super::ChangesView;
use crate::git::change_tree::{
    FileViewMode, TreeFile, TreeNode, TreeRow, all_folder_keys, ancestor_keys, build_change_tree,
    entries_under, file_order, flatten_tree, list_rows,
};

#[derive(Default)]
pub(super) struct Kept {
    tree: OnceCell<Vec<TreeNode>>,
    tree_order: OnceCell<Rc<[String]>>,
    list_order: OnceCell<Rc<[String]>>,
    folder_keys: OnceCell<Vec<String>>,
    rows: Option<KeptRows>,
    /// Row builds, for the tests.
    #[cfg(test)]
    builds: usize,
}

/// The rows and what they were built from.
struct KeptRows {
    mode: FileViewMode,
    folded: Option<BTreeSet<String>>,
    picked: Option<String>,
    rows: Rc<Vec<RowFacts>>,
}

impl Kept {
    pub(super) fn tree(&self, list: &[ChangeEntry]) -> &[TreeNode] {
        self.tree.get_or_init(|| build_change_tree(list, true))
    }

    pub(super) fn folder_keys(&self, list: &[ChangeEntry]) -> &[String] {
        self.folder_keys
            .get_or_init(|| all_folder_keys(self.tree(list)))
    }

    fn order(&self, list: &[ChangeEntry], mode: FileViewMode) -> Rc<[String]> {
        let kept = match mode {
            FileViewMode::Tree => &self.tree_order,
            FileViewMode::List => &self.list_order,
        };
        kept.get_or_init(|| Rc::from(file_order(list, mode)))
            .clone()
    }
}

/// One row as drawn: what it shows and what a click does.
#[derive(Clone)]
pub(super) struct RowFacts {
    pub key: String,
    pub depth: usize,
    pub node: RowNode,
    pub expanded: bool,
}

/// A row's file, or its folder without the folder's children.
#[derive(Clone)]
pub(super) enum RowNode {
    File(TreeFile),
    Folder(FolderFacts),
}

#[derive(Clone)]
pub(super) struct FolderFacts {
    /// The folder's path, the tree's key.
    pub key: String,
    pub name: String,
    pub file_count: u64,
    pub added: Option<u64>,
    pub removed: Option<u64>,
    pub status: ChangeStatus,
}

impl RowNode {
    pub fn name(&self) -> &str {
        match self {
            Self::File(file) => &file.name,
            Self::Folder(folder) => &folder.name,
        }
    }
}

impl RowFacts {
    fn of(row: TreeRow<'_>) -> Self {
        let node = match row.node.as_ref() {
            TreeNode::File(file) => RowNode::File(file.clone()),
            TreeNode::Folder(folder) => RowNode::Folder(FolderFacts {
                key: folder.key.clone(),
                name: folder.name.clone(),
                file_count: folder.file_count,
                added: folder.added,
                removed: folder.removed,
                status: folder.status,
            }),
        };
        RowFacts {
            key: row.key,
            depth: row.level - 1,
            node,
            expanded: row.expanded,
        }
    }
}

/// The folder node with the folder path `key`, at any depth.
fn find_folder<'a>(nodes: &'a [TreeNode], key: &str) -> Option<&'a TreeNode> {
    nodes.iter().find_map(|node| match node {
        TreeNode::Folder(folder) if folder.key == key => Some(node),
        TreeNode::Folder(folder) if key.starts_with(&format!("{}/", folder.key)) => {
            find_folder(&folder.children, key)
        }
        _ => None,
    })
}

impl ChangesView {
    /// The files in the order of the panel: depth first in the tree, as listed otherwise.
    pub(in crate::git) fn order(&self, cx: &App) -> Rc<[String]> {
        match self.list.as_deref() {
            Some(list) => self.kept.order(list, self.mode(cx)),
            None => Rc::from(Vec::<String>::new()),
        }
    }

    /// The file the diff shows: the picked one, else the first of the panel.
    pub(in crate::git) fn shown_path(&self, cx: &App) -> Option<String> {
        self.picked
            .clone()
            .or_else(|| self.order(cx).first().cloned())
    }

    pub(in crate::git) fn shown_entry(&self, cx: &App) -> Option<ChangeEntry> {
        let path = self.shown_path(cx)?;
        self.list
            .as_ref()?
            .iter()
            .find(|entry| entry.path == path)
            .cloned()
    }

    /// The rows of the panel, built again only when what they show changed.
    pub(super) fn rows(&mut self, cx: &App) -> Rc<Vec<RowFacts>> {
        let mode = self.mode(cx);
        // The picked file opens its folders, which only the tree has.
        let picked = self.picked.clone().filter(|_| mode == FileViewMode::Tree);
        let folded = self.task_id.as_ref().and_then(|id| self.folded.get(id));
        if let Some(kept) = &self.kept.rows
            && kept.mode == mode
            && kept.picked == picked
            && kept.folded.as_ref() == folded
        {
            return kept.rows.clone();
        }
        let rows = Rc::new(self.build_rows(mode));
        let folded = folded.cloned();
        self.kept.rows = Some(KeptRows {
            mode,
            folded,
            picked,
            rows: rows.clone(),
        });
        #[cfg(test)]
        {
            self.kept.builds += 1;
        }
        rows
    }

    fn build_rows(&self, mode: FileViewMode) -> Vec<RowFacts> {
        let Some(list) = self.list.as_deref() else {
            return Vec::new();
        };
        match mode {
            FileViewMode::List => list_rows(list).into_iter().map(RowFacts::of).collect(),
            FileViewMode::Tree => {
                let folded = self.folded_now();
                flatten_tree(self.kept.tree(list), &folded)
                    .into_iter()
                    .map(RowFacts::of)
                    .collect()
            }
        }
    }

    /// The folded folders of the task on screen; the picked file's folders are always open.
    pub(super) fn folded_now(&self) -> BTreeSet<String> {
        let mut folded = self
            .task_id
            .as_ref()
            .and_then(|id| self.folded.get(id))
            .cloned()
            .unwrap_or_default();
        if let (Some(list), Some(path)) = (self.list.as_deref(), self.picked.as_deref()) {
            for key in ancestor_keys(self.kept.tree(list), path) {
                folded.remove(&key);
            }
        }
        folded
    }

    /// The changed files a row stands for: its file, or every file under its folder.
    pub(super) fn row_entries(&self, node: &RowNode) -> Vec<ChangeEntry> {
        match node {
            RowNode::File(file) => vec![file.entry.clone()],
            RowNode::Folder(folder) => self
                .list
                .as_deref()
                .and_then(|list| find_folder(self.kept.tree(list), &folder.key))
                .map(entries_under)
                .unwrap_or_default(),
        }
    }
}

#[cfg(test)]
#[path = "changes_kept_tests.rs"]
mod tests;
