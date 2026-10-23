//! What the Files tab reads (`FileTree.tsx`'s queries): the root, each folder once when it opens,
//! and the changes of the task. A read that fails keeps what is shown; only the root's first
//! read shows its error. The open folders of each task are kept while the app runs.

use std::collections::{BTreeSet, HashMap};

use gpui::{App, Context, Global};
use natamux_proto::workspaces::files::TreeEntry;
use natamux_proto::workspaces::workspace::ChangeEntry;

use super::explorer_tree::shown_folders;
use super::file_tree::FileTree;
use crate::app::source::reason;

#[derive(Default)]
struct OpenFolders(HashMap<String, BTreeSet<String>>);

impl Global for OpenFolders {}

/// The folders the user opened in a task's tree.
pub fn open_folders(task_id: &str, cx: &App) -> BTreeSet<String> {
    cx.try_global::<OpenFolders>()
        .and_then(|open| open.0.get(task_id))
        .cloned()
        .unwrap_or_default()
}

pub fn keep_open_folders(task_id: &str, folders: &BTreeSet<String>, cx: &mut App) {
    cx.default_global::<OpenFolders>()
        .0
        .insert(task_id.to_owned(), folders.clone());
}

/// The rows of a task's folders and its changed files, as last read.
#[derive(Default)]
pub struct FileReads {
    pub folders: HashMap<String, Vec<TreeEntry>>,
    pub changes: Vec<ChangeEntry>,
    /// The root's error while it has never been read.
    pub root_error: Option<String>,
    /// Folders a read was started for, so each opens with one read.
    asked: BTreeSet<String>,
}

impl FileReads {
    fn apply_folder(&mut self, folder: &str, result: Result<Vec<TreeEntry>, String>) {
        match result {
            Ok(entries) => {
                self.folders.insert(folder.to_owned(), entries);
                if folder.is_empty() {
                    self.root_error = None;
                }
            }
            Err(message) => {
                // The next open of this folder reads it again.
                self.asked.remove(folder);
                if folder.is_empty() && !self.folders.contains_key("") {
                    self.root_error = Some(message);
                }
            }
        }
    }
}

impl FileTree {
    /// Reads each shown folder that was never asked for.
    pub(super) fn read_new_folders(&mut self, cx: &mut Context<Self>) {
        for folder in shown_folders(&self.expanded) {
            if self.reads.asked.insert(folder.clone()) {
                self.read_folder(folder, cx);
            }
        }
    }

    /// Refresh and `workspace.dirty`: every shown folder and the changes again. `fresh` skips
    /// the daemon's cache of the changes (the Refresh button).
    pub(super) fn read_all(&mut self, fresh: bool, cx: &mut Context<Self>) {
        for folder in shown_folders(&self.expanded) {
            self.reads.asked.insert(folder.clone());
            self.read_folder(folder, cx);
        }
        self.read_changes(fresh, cx);
    }

    fn read_folder(&mut self, folder: String, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.list_folder(&self.task_id, &folder);
        cx.spawn(async move |tree, cx| {
            let result = answer.await.map_err(|error| reason(&error));
            tree.update(cx, |tree, cx| {
                tree.reads.apply_folder(&folder, result);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn read_changes(&mut self, fresh: bool, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.read_changes(&self.task_id, fresh);
        cx.spawn(async move |tree, cx| {
            // A failed read keeps the marks shown.
            if let Ok(changes) = answer.await {
                tree.update(cx, |tree, cx| {
                    tree.reads.changes = changes;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }
}
