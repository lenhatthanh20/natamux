//! The pure rules of the files explorer (`features/files/explorer-tree.ts`, the folder rules of
//! `FileTree.tsx`, the tree keys of `lib/roving.ts`): the rows, the change marks, the text of a
//! drop on a terminal.

use std::collections::{BTreeSet, HashMap, HashSet};

use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::files::{EntryKind, TreeEntry};
use natamux_proto::workspaces::workspace::ChangeEntry;

use crate::terminals::paste_rules::quote_path;

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

/// What Copy Path and Copy Relative Path give for the worktree path `key`: the full path in the
/// task's folder with the platform's separator (None while the folder is not known), and the
/// path in the worktree. Neither ends with a separator.
pub fn copy_paths(workspace: Option<&str>, key: &str) -> (Option<String>, String) {
    let relative = key.trim_matches('/').to_owned();
    let full = workspace.filter(|folder| !folder.is_empty()).map(|folder| {
        if relative.is_empty() {
            let trimmed = folder.trim_end_matches(['/', '\\']);
            return if trimmed.is_empty() { folder } else { trimmed }.to_string();
        }
        let inside = relative.replace('/', std::path::MAIN_SEPARATOR_STR);
        std::path::Path::new(folder)
            .join(inside)
            .to_string_lossy()
            .into_owned()
    });
    (full, relative)
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
    drop_text_for(path, kind, cfg!(windows))
}

/// `drop_text` for a chosen shell: `windows` picks PowerShell quoting, else POSIX. Control
/// characters are removed so a file name cannot type keys (Enter, ^C) into the terminal.
pub fn drop_text_for(path: &str, kind: EntryKind, windows: bool) -> String {
    let mut text: String = path.chars().filter(|c| *c >= ' ' && *c != '\x7f').collect();
    if kind == EntryKind::Folder {
        text.push('/');
    }
    format!("{} ", quote_path(&text, windows))
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
