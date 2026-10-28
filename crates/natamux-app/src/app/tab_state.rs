//! What main shows for the open workspace (`changesTab`, `rememberTab`, `openChanges`,
//! `showChangesOf`, `openFile` and `closeFile` of the web): the terminals, or Changes, Docs or the
//! open file over them (T-0183: no tab bar; the panel's rail picks them). What the user picks is
//! kept per task in `view.json`, except the open file.

use ui::prelude::*;

use super::Dashboard;
use crate::app::tab_rules::shown_tab;
use crate::app::view_rules::{DetailsView, WorkspaceTab};
use crate::app::view_state::DiffFlags;
pub use crate::files::open_files::FileViews;
use crate::git::change_tree::FileViewMode;
use crate::workspaces::workspace_rules::{shown_after_remember, tab_to_open};

impl Dashboard {
    /// The screen the workspace of `task` opens on: its terminals while its agent works or
    /// waits, else the one the user used last in it.
    pub(super) fn tab_on_open(&self, id: &str) -> WorkspaceTab {
        let saved = self.view.tab_of(id);
        let state = self.data.task(id).map(|task| task.state);
        state.map_or(WorkspaceTab::Terminal, |state| tab_to_open(state, saved))
    }

    /// Mod+W: the active file closes, its draft stays. The other screens stay.
    pub fn close_open_file(&mut self, id: &str, cx: &mut Context<Self>) {
        if let Some(path) = self.files.editor_file(id).map(String::from) {
            self.close_file(id, &path, cx);
        }
    }

    /// Key 1, Terminals, Esc, a pick in the Changes or Docs list, History's "N files not
    /// committed.": the open workspace shows `tab`, and its task remembers it. The editor needs
    /// an open file.
    pub fn pick_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        let Some(id) = self.open_task.clone() else {
            return;
        };
        if shown_tab(tab, self.files.editor_file(&id).is_some()) != tab {
            return;
        }
        self.workspace_tab =
            shown_after_remember(self.open_task.as_deref(), self.workspace_tab, &id, tab);
        self.update_view(cx, |view| view.remember_tab(&id, tab));
    }

    /// The count of files to commit in the open task's workspace, as the workspace last read
    /// it: the badge of the rail's Changes.
    pub fn open_changes_of(&self, id: &str) -> Option<usize> {
        self.open_changes
            .as_ref()
            .filter(|(task, _)| task == id)
            .map(|(_, count)| *count)
    }

    /// The workspace read the changes of `id`: draws the rail again when the count moved.
    pub fn set_open_changes(&mut self, id: &str, count: usize, cx: &mut Context<Self>) {
        if self.open_changes_of(id) != Some(count) {
            self.open_changes = Some((id.to_owned(), count));
            cx.notify();
        }
    }

    /// View changes and Commit… (`showChangesOf`): the task's workspace with the panel on
    /// Changes and the diff over main. With `commit`, the commit box drafts a message when it is
    /// empty, and takes the focus.
    pub fn open_changes(&mut self, id: &str, commit: bool, cx: &mut Context<Self>) {
        self.open(id, cx);
        if self.open_task.as_deref() != Some(id) {
            return;
        }
        self.show_view(DetailsView::Changes, cx);
        if commit {
            self.commit_request = Some(id.to_owned());
        }
        cx.notify();
    }

    /// The tab the open workspace shows: the editor tab is nothing without its file.
    pub fn shown_workspace_tab(&self) -> WorkspaceTab {
        let file_open = self
            .open_task
            .as_deref()
            .is_some_and(|id| self.files.editor_file(id).is_some());
        shown_tab(self.workspace_tab, file_open)
    }

    /// A file of the Files tab or a click on its tab (`openFile`): the task's workspace on the
    /// editor tab with it.
    pub fn open_file(&mut self, id: &str, path: &str, cx: &mut Context<Self>) {
        if self.open_task.as_deref() != Some(id) {
            self.open(id, cx);
        }
        if self.open_task.as_deref() != Some(id) {
            return;
        }
        self.workspace_tab = self.files.open(id, path);
        cx.notify();
    }

    /// `closeFile`: the file's tab closes. After the last one, back to the tab the user picked
    /// last in the workspace, or the terminal.
    pub fn close_file(&mut self, id: &str, path: &str, cx: &mut Context<Self>) {
        let shown = (self.open_task.as_deref() == Some(id)).then_some(self.workspace_tab);
        if let Some(tab) = self.files.close(id, path, shown, self.view.tab_of(id)) {
            self.workspace_tab = tab;
        }
        cx.notify();
    }

    /// The output chip of a stage card (`openStageFile`): the task's workspace with the panel on
    /// Docs and that file over main.
    pub fn open_stage_file(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        if self.open_task.as_deref() != Some(id) {
            self.open(id, cx);
        }
        if self.open_task.as_deref() != Some(id) {
            return;
        }
        self.files.request_doc(id, name);
        self.show_view(DetailsView::Docs, cx);
        cx.notify();
    }

    /// The Docs tab takes the file picked for its task, once.
    pub fn take_doc_request(&mut self, id: &str) -> Option<String> {
        self.files.take_doc_request(id)
    }

    /// The Changes tab takes the Commit… request meant for its task, once.
    pub fn take_commit_request(&mut self, id: &str) -> bool {
        let ours = self.commit_request.as_deref() == Some(id);
        if ours {
            self.commit_request = None;
        }
        ours
    }

    pub fn set_changes_view(&mut self, mode: FileViewMode, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.changes_view = mode);
    }

    pub fn set_files_width(&mut self, width: f32, cx: &mut Context<Self>) {
        self.update_view(cx, |view| view.set_files_width(width.round() as u32));
    }

    /// One of the diff toggles of the toolbar, kept for every workspace.
    pub fn set_diff_flags(&mut self, change: impl FnOnce(&mut DiffFlags), cx: &mut Context<Self>) {
        self.update_view(cx, |view| change(&mut view.diff));
    }
}
