//! The changed files as a list or as folders (`features/git/change-tree.ts`), and how the file
//! panel shows them. Pure, so the panel only draws what this says.

// The Changes tab uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

use natamux_proto::workspaces::changes::ChangeStatus;
use natamux_proto::workspaces::workspace::ChangeEntry;

/// How the Changes tab shows its files. Kept in `view.json` as `changes_view`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FileViewMode {
    #[default]
    List,
    Tree,
}

impl FileViewMode {
    pub fn name(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Tree => "tree",
        }
    }

    /// Any value but `tree` reads as the list.
    pub fn parse(value: &str) -> Self {
        if value == "tree" {
            Self::Tree
        } else {
            Self::List
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LineCounts {
    pub added: u64,
    pub removed: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeFolder {
    /// The full path of the deepest folder in the row, like `packages/web/src`.
    pub key: String,
    /// The label, relative to the parent. Compacted folders join with `/`, like `web/src`.
    pub name: String,
    pub depth: usize,
    pub children: Vec<TreeNode>,
    /// Files below, at any depth.
    pub file_count: u64,
    pub added: Option<u64>,
    pub removed: Option<u64>,
    /// The strongest status below, so a folder shows what needs the most care.
    pub status: ChangeStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeFile {
    pub key: String,
    pub name: String,
    pub depth: usize,
    pub entry: ChangeEntry,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TreeNode {
    Folder(TreeFolder),
    File(TreeFile),
}

impl TreeNode {
    pub fn name(&self) -> &str {
        match self {
            Self::Folder(folder) => &folder.name,
            Self::File(file) => &file.name,
        }
    }

    pub fn key(&self) -> &str {
        match self {
            Self::Folder(folder) => &folder.key,
            Self::File(file) => &file.key,
        }
    }

    pub fn depth(&self) -> usize {
        match self {
            Self::Folder(folder) => folder.depth,
            Self::File(file) => file.depth,
        }
    }
}

/// One row on screen.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TreeRow<'a> {
    /// `d:` and a folder key, or `f:` and a path.
    pub key: String,
    pub node: Cow<'a, TreeNode>,
    pub parent: Option<String>,
    /// 1-based, like `aria-level`.
    pub level: usize,
    pub pos_in_set: usize,
    pub set_size: usize,
    pub expandable: bool,
    pub expanded: bool,
}

/// None when the daemon could not count (an older daemon, a binary or a big file).
pub fn entry_counts(entry: &ChangeEntry) -> Option<LineCounts> {
    let added = entry.added?;
    Some(LineCounts {
        added,
        removed: entry.removed.unwrap_or(0),
    })
}

/// The sum over the entries that have counts, or None when none has.
pub fn total_counts(list: &[ChangeEntry]) -> Option<LineCounts> {
    list.iter()
        .filter_map(entry_counts)
        .fold(None, |sum, counts| {
            let sum = sum.unwrap_or(LineCounts {
                added: 0,
                removed: 0,
            });
            Some(LineCounts {
                added: sum.added + counts.added,
                removed: sum.removed + counts.removed,
            })
        })
}

fn digit_run(chars: &[char], from: usize) -> usize {
    chars[from..]
        .iter()
        .position(|c| !c.is_ascii_digit())
        .map_or(chars.len(), |n| from + n)
}

/// Natural order (`file2` before `file10`), case-insensitive. Names that compare equal, like
/// `a.ts` and `A.ts`, fall back to code points so the order never flips between reads. Written
/// by hand: the app has no ICU collator.
pub fn compare_names(a: &str, b: &str) -> Ordering {
    let lower = |text: &str| -> Vec<char> { text.chars().flat_map(char::to_lowercase).collect() };
    let (left, right) = (lower(a), lower(b));
    let (mut i, mut j) = (0, 0);
    while i < left.len() && j < right.len() {
        let order = if left[i].is_ascii_digit() && right[j].is_ascii_digit() {
            let (end_i, end_j) = (digit_run(&left, i), digit_run(&right, j));
            let trim = |run: &[char]| -> usize { run.iter().take_while(|c| **c == '0').count() };
            let x = &left[i + trim(&left[i..end_i])..end_i];
            let y = &right[j + trim(&right[j..end_j])..end_j];
            i = end_i;
            j = end_j;
            x.len().cmp(&y.len()).then_with(|| x.cmp(y))
        } else {
            let order = left[i].cmp(&right[j]);
            i += 1;
            j += 1;
            order
        };
        if order != Ordering::Equal {
            return order;
        }
    }
    (left.len() - i)
        .cmp(&(right.len() - j))
        .then_with(|| a.cmp(b))
}

fn status_rank(status: ChangeStatus) -> u8 {
    match status {
        ChangeStatus::Conflict => 6,
        ChangeStatus::Deleted => 5,
        ChangeStatus::Modified => 4,
        ChangeStatus::Renamed => 3,
        ChangeStatus::Added => 2,
        ChangeStatus::Untracked => 1,
    }
}

fn stronger(a: ChangeStatus, b: ChangeStatus) -> ChangeStatus {
    if status_rank(b) > status_rank(a) {
        b
    } else {
        a
    }
}

fn add_count(sum: Option<u64>, value: Option<u64>) -> Option<u64> {
    match value {
        Some(value) => Some(sum.unwrap_or(0) + value),
        None => sum,
    }
}

pub fn row_key(node: &TreeNode) -> String {
    match node {
        TreeNode::Folder(folder) => format!("d:{}", folder.key),
        TreeNode::File(file) => format!("f:{}", file.key),
    }
}

#[derive(Default)]
struct Dir<'a> {
    name: String,
    path: String,
    dirs: BTreeMap<String, Dir<'a>>,
    files: Vec<&'a ChangeEntry>,
}

fn file_node(entry: &ChangeEntry, depth: usize) -> TreeFile {
    let name = entry.path.rsplit('/').next().unwrap_or(&entry.path);
    TreeFile {
        key: entry.path.clone(),
        name: name.to_string(),
        depth,
        entry: entry.clone(),
    }
}

fn nodes_of(dir: &Dir, depth: usize, compact: bool) -> Vec<TreeNode> {
    let mut folders: Vec<&Dir> = dir.dirs.values().collect();
    folders.sort_by(|a, b| compare_names(&a.name, &b.name));
    let mut files: Vec<TreeFile> = dir.files.iter().map(|e| file_node(e, depth)).collect();
    files.sort_by(|a, b| compare_names(&a.name, &b.name));
    folders
        .into_iter()
        .map(|child| TreeNode::Folder(folder_of(child, depth, compact)))
        .chain(files.into_iter().map(TreeNode::File))
        .collect()
}

fn folder_of(start: &Dir, depth: usize, compact: bool) -> TreeFolder {
    let mut dir = start;
    let mut names = vec![start.name.as_str()];
    // A folder that holds only one folder adds a row that says nothing, so the two share a row.
    while compact && dir.files.is_empty() && dir.dirs.len() == 1 {
        let Some(only) = dir.dirs.values().next() else {
            break;
        };
        dir = only;
        names.push(&only.name);
    }
    let children = nodes_of(dir, depth + 1, compact);
    let mut folder = TreeFolder {
        key: dir.path.clone(),
        name: names.join("/"),
        depth,
        children: Vec::new(),
        file_count: 0,
        added: None,
        removed: None,
        status: ChangeStatus::Untracked,
    };
    for child in &children {
        match child {
            TreeNode::Folder(sub) => {
                folder.file_count += sub.file_count;
                folder.added = add_count(folder.added, sub.added);
                folder.removed = add_count(folder.removed, sub.removed);
                folder.status = stronger(folder.status, sub.status);
            }
            TreeNode::File(file) => {
                folder.file_count += 1;
                folder.added = add_count(folder.added, file.entry.added);
                folder.removed = add_count(folder.removed, file.entry.removed);
                folder.status = stronger(folder.status, file.entry.status);
            }
        }
    }
    folder.children = children;
    folder
}

/// The changed files as folders, like an editor's explorer. A renamed file sits at its new path.
pub fn build_change_tree(list: &[ChangeEntry], compact: bool) -> Vec<TreeNode> {
    let mut root = Dir::default();
    for entry in list {
        let mut dir = &mut root;
        let parts: Vec<&str> = entry.path.split('/').collect();
        for part in &parts[..parts.len() - 1] {
            let path = if dir.path.is_empty() {
                (*part).to_string()
            } else {
                format!("{}/{part}", dir.path)
            };
            dir = dir.dirs.entry((*part).to_string()).or_insert_with(|| Dir {
                name: (*part).to_string(),
                path,
                ..Dir::default()
            });
        }
        dir.files.push(entry);
    }
    nodes_of(&root, 0, compact)
}

fn walk<'a>(
    siblings: &'a [TreeNode],
    parent: Option<&str>,
    level: usize,
    collapsed: &BTreeSet<String>,
    rows: &mut Vec<TreeRow<'a>>,
) {
    for (index, node) in siblings.iter().enumerate() {
        let key = row_key(node);
        let folder = match node {
            TreeNode::Folder(folder) => Some(folder),
            TreeNode::File(_) => None,
        };
        let expanded = folder.is_some() && !collapsed.contains(&key);
        rows.push(TreeRow {
            key: key.clone(),
            node: Cow::Borrowed(node),
            parent: parent.map(str::to_string),
            level,
            pos_in_set: index + 1,
            set_size: siblings.len(),
            expandable: folder.is_some(),
            expanded,
        });
        if let (Some(folder), true) = (folder, expanded) {
            walk(&folder.children, Some(&key), level + 1, collapsed, rows);
        }
    }
}

/// The rows on screen. `collapsed` holds row keys (`d:` and a folder key); their children are
/// left out.
pub fn flatten_tree<'a>(nodes: &'a [TreeNode], collapsed: &BTreeSet<String>) -> Vec<TreeRow<'a>> {
    let mut rows = Vec::new();
    walk(nodes, None, 1, collapsed, &mut rows);
    rows
}

/// The rows of the flat list, in the order the daemon gave.
pub fn list_rows(list: &[ChangeEntry]) -> Vec<TreeRow<'static>> {
    list.iter()
        .enumerate()
        .map(|(index, entry)| {
            let node = TreeNode::File(file_node(entry, 0));
            TreeRow {
                key: row_key(&node),
                node: Cow::Owned(node),
                parent: None,
                level: 1,
                pos_in_set: index + 1,
                set_size: list.len(),
                expandable: false,
                expanded: false,
            }
        })
        .collect()
}

/// The row keys of the folders around a path, outermost first.
pub fn ancestor_keys(nodes: &[TreeNode], path: &str) -> Vec<String> {
    let folder = nodes.iter().find_map(|node| match node {
        TreeNode::Folder(folder) if path.starts_with(&format!("{}/", folder.key)) => Some(folder),
        _ => None,
    });
    match folder {
        None => Vec::new(),
        Some(folder) => {
            let mut keys = vec![row_key(&TreeNode::Folder(folder.clone()))];
            keys.extend(ancestor_keys(&folder.children, path));
            keys
        }
    }
}

pub fn all_folder_keys(nodes: &[TreeNode]) -> Vec<String> {
    let mut keys = Vec::new();
    for node in nodes {
        if let TreeNode::Folder(folder) = node {
            keys.push(format!("d:{}", folder.key));
            keys.extend(all_folder_keys(&folder.children));
        }
    }
    keys
}

/// The changed files a row stands for: its file, or every file under its folder.
pub fn entries_under(node: &TreeNode) -> Vec<ChangeEntry> {
    match node {
        TreeNode::File(file) => vec![file.entry.clone()],
        TreeNode::Folder(folder) => folder.children.iter().flat_map(entries_under).collect(),
    }
}

fn file_paths(nodes: &[TreeNode], into: &mut Vec<String>) {
    for node in nodes {
        match node {
            TreeNode::Folder(folder) => file_paths(&folder.children, into),
            TreeNode::File(file) => into.push(file.key.clone()),
        }
    }
}

/// The paths in the order the panel shows them, for `[` and `]`. Collapsed folders count too:
/// stepping into one opens it.
pub fn file_order(list: &[ChangeEntry], mode: FileViewMode) -> Vec<String> {
    match mode {
        FileViewMode::Tree => {
            let mut paths = Vec::new();
            file_paths(&build_change_tree(list, true), &mut paths);
            paths
        }
        FileViewMode::List => list.iter().map(|entry| entry.path.clone()).collect(),
    }
}

#[cfg(test)]
#[path = "change_tree_tests.rs"]
mod tests;
