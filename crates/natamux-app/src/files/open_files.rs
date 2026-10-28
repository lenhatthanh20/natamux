//! The files open in the editor screen and the stage file the Docs tab should pick, while the
//! app runs (`editorFileByTask`, `openFile`, `closeFile` and the `open-doc` event of the web).
//! Each task keeps its open files as tabs, in the order they opened, one of them active (T-0220).

use crate::app::view_rules::WorkspaceTab;
use crate::git::change_rules::split_path;

struct OpenFiles {
    task_id: String,
    paths: Vec<String>,
    active: String,
}

#[derive(Default)]
pub struct FileViews {
    editor: Vec<OpenFiles>,
    doc_request: Option<(String, String)>,
}

impl FileViews {
    fn of(&self, task_id: &str) -> Option<&OpenFiles> {
        self.editor.iter().find(|open| open.task_id == task_id)
    }

    /// The active file of the task: the one the editor shows.
    pub fn editor_file(&self, task_id: &str) -> Option<&str> {
        self.of(task_id).map(|open| open.active.as_str())
    }

    /// The open files of the task, in the order of their tabs.
    pub fn editor_files(&self, task_id: &str) -> &[String] {
        self.of(task_id).map_or(&[], |open| open.paths.as_slice())
    }

    /// `openFile`: the file is active, with a new tab at the end when it was not open; the
    /// task's workspace shows the editor tab.
    pub fn open(&mut self, task_id: &str, path: &str) -> WorkspaceTab {
        match self.editor.iter_mut().find(|open| open.task_id == task_id) {
            Some(open) => {
                if !open.paths.iter().any(|known| known == path) {
                    open.paths.push(path.to_owned());
                }
                open.active = path.to_owned();
            }
            None => self.editor.push(OpenFiles {
                task_id: task_id.to_owned(),
                paths: vec![path.to_owned()],
                active: path.to_owned(),
            }),
        }
        WorkspaceTab::Editor
    }

    /// `closeFile`: the file's tab closes. The active one hands over to the tab now at its place,
    /// else the one before. When the last tab closed, `shown` is the tab of the task's open
    /// workspace (None: it is not open) and `remembered` the tab the user picked last in it;
    /// gives the tab to show instead: the picked one, or the terminal.
    pub fn close(
        &mut self,
        task_id: &str,
        path: &str,
        shown: Option<WorkspaceTab>,
        remembered: Option<WorkspaceTab>,
    ) -> Option<WorkspaceTab> {
        let at = self
            .editor
            .iter()
            .position(|open| open.task_id == task_id)?;
        let open = &mut self.editor[at];
        let index = open.paths.iter().position(|known| known == path)?;
        open.paths.remove(index);
        if open.active == path {
            let next = index.min(open.paths.len().saturating_sub(1));
            if let Some(next) = open.paths.get(next).cloned() {
                open.active = next;
            }
        }
        if !open.paths.is_empty() {
            return None;
        }
        self.editor.remove(at);
        (shown == Some(WorkspaceTab::Editor)).then(|| remembered.unwrap_or(WorkspaceTab::Terminal))
    }

    pub fn request_doc(&mut self, task_id: &str, name: &str) {
        self.doc_request = Some((task_id.to_owned(), name.to_owned()));
    }

    /// The Docs tab of the task takes its file once.
    pub fn take_doc_request(&mut self, task_id: &str) -> Option<String> {
        let ours = self
            .doc_request
            .as_ref()
            .is_some_and(|(task, _)| task == task_id);
        ours.then(|| self.doc_request.take().map(|(_, name)| name))
            .flatten()
    }
}

/// The label of each tab: the file name, and its folder when another open file has the same
/// name (`mod.rs` in two folders).
pub fn tab_labels(paths: &[String]) -> Vec<(String, Option<String>)> {
    paths
        .iter()
        .map(|path| {
            let (folder, name) = split_path(path);
            let shared = paths
                .iter()
                .filter(|other| split_path(other).1 == name)
                .count()
                > 1;
            let folder = folder.trim_end_matches('/');
            let folder = (shared && !folder.is_empty()).then(|| folder.to_owned());
            (name.to_owned(), folder)
        })
        .collect()
}

#[cfg(test)]
#[path = "open_files_tests.rs"]
mod tests;
