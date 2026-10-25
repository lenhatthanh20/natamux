//! The pure rules of the files explorer (`features/files/explorer-tree.ts`, the folder rules of
//! `FileTree.tsx`, the tree keys of `lib/roving.ts`): the rows, the change marks, the text of a
//! drop on a terminal.

use std::collections::{BTreeSet, HashMap, HashSet};

use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::files::{EntryKind, TreeEntry};
use natamux_proto::workspaces::workspace::ChangeEntry;

/// One row of the explorer. The key is the path in the worktree. Level and place are 1-based.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExplorerRow {
    pub key: String,
    pub name: String,
    pub kind: EntryKind,
    pub parent: Option<String>,
    pub level: usize,
    pub pos_in_set: usize,
    pub set_size: usize,
    pub expandable: bool,
    pub expanded: bool,
}

pub fn join_path(folder: &str, name: &str) -> String {
    if folder.is_empty() {
        name.to_string()
    } else {
        format!("{folder}/{name}")
    }
}

/// The rows on screen: the root, then the children of each open folder that was read. "" is the
/// root.
pub fn explorer_rows(
    folders: &HashMap<String, Vec<TreeEntry>>,
    expanded: &BTreeSet<String>,
) -> Vec<ExplorerRow> {
    let mut rows = Vec::new();
    walk(folders, expanded, "", 1, &mut rows);
    rows
}

fn walk(
    folders: &HashMap<String, Vec<TreeEntry>>,
    expanded: &BTreeSet<String>,
    folder: &str,
    level: usize,
    rows: &mut Vec<ExplorerRow>,
) {
    let Some(entries) = folders.get(folder) else {
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let key = join_path(folder, &entry.name);
        let is_folder = entry.kind == EntryKind::Folder;
        let open = is_folder && expanded.contains(&key);
        rows.push(ExplorerRow {
            key: key.clone(),
            name: entry.name.clone(),
            kind: entry.kind,
            parent: (!folder.is_empty()).then(|| folder.to_string()),
            level,
            pos_in_set: index + 1,
            set_size: entries.len(),
            expandable: is_folder,
            expanded: open,
        });
        if open {
            walk(folders, expanded, &key, level + 1, rows);
        }
    }
}

/// The status of each changed file, and the folders that hold one at any depth.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ChangeMarks {
    pub files: HashMap<String, ChangeStatus>,
    pub folders: HashSet<String>,
}

pub fn change_marks(changes: &[ChangeEntry]) -> ChangeMarks {
    let mut marks = ChangeMarks::default();
    for change in changes {
        marks.files.insert(change.path.clone(), change.status);
        marks.folders.extend(folders_above(&change.path));
    }
    marks
}

/// The folders around a path, outermost first: `a/b/c.ts` gives `a`, `a/b`.
pub fn folders_above(path: &str) -> Vec<String> {
    let parts: Vec<&str> = path.split('/').collect();
    (1..parts.len()).map(|end| parts[..end].join("/")).collect()
}

/// The folders to read: the root, then each open folder whose folders above are all open too. An
/// open folder inside a closed one waits.
pub fn shown_folders(expanded: &BTreeSet<String>) -> Vec<String> {
    let mut shown = vec![String::new()];
    shown.extend(
        expanded
            .iter()
            .filter(|path| {
                folders_above(path)
                    .iter()
                    .all(|above| expanded.contains(above))
            })
            .cloned(),
    );
    shown
}

/// What a drop on a terminal types: the path from the worktree root (a folder ends with `/`),
/// quoted when a shell would split or expand it, then a space. No Enter.
pub fn drop_text(path: &str, kind: EntryKind) -> String {
    let text = if kind == EntryKind::Folder {
        format!("{path}/")
    } else {
        path.to_string()
    };
    let plain = !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "._/@+-".contains(c));
    if plain {
        format!("{text} ")
    } else {
        format!("'{}' ", text.replace('\'', "'\\''"))
    }
}

/// What a row drags: the task is in it, so a terminal of another task can refuse before the drop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DraggedPath {
    pub task_id: String,
    pub path: String,
    pub kind: EntryKind,
}

impl DraggedPath {
    pub fn accepts(&self, task_id: &str) -> bool {
        self.task_id.eq_ignore_ascii_case(task_id)
    }
}

/// The key a row goes to with Left, when it is not an open folder.
pub fn parent_key(rows: &[ExplorerRow], key: &str) -> Option<String> {
    rows.iter().find(|row| row.key == key)?.parent.clone()
}

/// The first child of a folder, where Right goes when the folder is open.
pub fn first_child_key(rows: &[ExplorerRow], key: &str) -> Option<String> {
    rows.iter()
        .find(|row| row.parent.as_deref() == Some(key))
        .map(|row| row.key.clone())
}

#[cfg(test)]
#[path = "explorer_tree_tests.rs"]
mod tests;
