//! The keys and menu items of screen 4 that act on the open workspace as a whole: the tabs and
//! Commit… The Changes view handles the diff keys itself.

use ui::prelude::*;

use super::Root;
use crate::app::shortcuts::{CommitChanges, ShowChangesTab, ShowHistoryTab, ShowTerminalTab};
use crate::app::view_rules::WorkspaceTab;

impl Root {
    /// The id of the open task, only when it has a workspace (`When::InWorkspace`).
    pub(super) fn open_workspace_task(&self, cx: &App) -> Option<String> {
        let dashboard = self.dashboard.read(cx);
        let id = dashboard.open_task.as_deref()?;
        dashboard.data.task(id)?.workspace.as_ref()?;
        Some(id.to_owned())
    }

    pub(super) fn show_tab(&mut self, tab: WorkspaceTab, cx: &mut Context<Self>) {
        if self.open_workspace_task(cx).is_some() {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.pick_tab(tab, cx));
        }
    }

    pub(super) fn show_terminal_tab(
        &mut self,
        _: &ShowTerminalTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_tab(WorkspaceTab::Terminal, cx);
    }

    pub(super) fn show_changes_tab(
        &mut self,
        _: &ShowChangesTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_tab(WorkspaceTab::Changes, cx);
    }

    pub(super) fn show_history_tab(
        &mut self,
        _: &ShowHistoryTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_tab(WorkspaceTab::History, cx);
    }

    pub(super) fn commit_changes(
        &mut self,
        _: &CommitChanges,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.open_workspace_task(cx) {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.open_changes(&id, true, cx));
        }
    }
}
