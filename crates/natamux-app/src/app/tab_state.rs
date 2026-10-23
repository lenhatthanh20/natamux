//! The tab of the open workspace (`changesTab`, `rememberTab`, `openChanges`, `showChangesOf`,
//! `openFile` and `closeFile` of the web): Terminal, Changes, History, Docs, and the editor tab
//! while a file is open. A tab the user picks is kept per task in `view.json`, except the editor.

use ui::prelude::*;

use super::Dashboard;
pub use crate::app::tab_rules::FileViews;
use crate::app::tab_rules::shown_tab;
use crate::app::view_rules::WorkspaceTab;
use crate::app::view_state::DiffFlags;
use crate::git::change_tree::FileViewMode;
use crate::workspaces::workspace_rules::{shown_after_remember, tab_to_open};

impl Dashboard {
    /// The tab the workspace of `task` opens on: its terminal while its agent works or waits,
    /// else the tab the user used last in it.
    pub(super) fn tab_on_open(&self, id: &str) -> WorkspaceTab {
        let saved = self.view.tab_of(id);
        let state = self.data.task(id).map(|task| task.state);
        state.map_or(WorkspaceTab::Terminal, |state| tab_to_open(state, saved))
    }

    /// Keys 1 to 4, a click on a tab, and History's "N files not committed.": the open
    /// workspace shows `tab`, and its task remembers it. The editor tab needs an open file.
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

    /// View changes and Commit… (`showChangesOf`): the task's workspace on its Changes tab.
    /// With `commit`, the commit box drafts a message when it is empty, and takes the focus.
    pub fn open_changes(&mut self, id: &str, commit: bool, cx: &mut Context<Self>) {
        self.open(id, cx);
        if self.open_task.as_deref() != Some(id) {
            return;
        }
        self.workspace_tab = WorkspaceTab::Changes;
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

    /// A file of the Files tab (`openFile`): the task's workspace on the editor tab with it.
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

    /// `closeFile`: back to the tab the user picked last in the workspace, or the terminal.
    pub fn close_file(&mut self, id: &str, cx: &mut Context<Self>) {
        let shown = (self.open_task.as_deref() == Some(id)).then_some(self.workspace_tab);
        if let Some(tab) = self.files.close(id, shown, self.view.tab_of(id)) {
            self.workspace_tab = tab;
        }
        cx.notify();
    }

    /// The output chip of a stage card (`openStageFile`): the task's workspace on Docs, with
    /// that file picked.
    pub fn open_stage_file(&mut self, id: &str, name: &str, cx: &mut Context<Self>) {
        if self.open_task.as_deref() != Some(id) {
            self.open(id, cx);
        }
        if self.open_task.as_deref() != Some(id) {
            return;
        }
        self.files.request_doc(id, name);
        self.pick_tab(WorkspaceTab::Docs, cx);
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
