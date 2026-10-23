//! The tab of the open workspace (`changesTab`, `rememberTab`, `openChanges` and
//! `showChangesOf` of the web): Terminal, Changes or History. A tab the user picks is kept per
//! task in `view.json`; Docs and the editor tab come with screen 6.

use ui::prelude::*;

use super::Dashboard;
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

    /// Keys 1 to 3, a click on a tab, and History's "N files not committed.": the open
    /// workspace shows `tab`, and its task remembers it.
    pub fn pick_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        let Some(id) = self.open_task.clone() else {
            return;
        };
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
