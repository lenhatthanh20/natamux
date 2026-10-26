//! The keys and menu items of screen 6 that act on the open workspace: the Docs tab, the open
//! file, the files of the worktree. And quitting while edits are not saved, which asks first, as
//! the browser does before it closes a page with a draft (`useWarnOnUnsavedFiles`).

use ui::prelude::*;

use super::{Root, Tool};
use crate::app::shortcuts::{CloseTab, Quit, ShowDocsTab, ShowOpenFile, ShowWorktreeFiles};
use crate::app::view_rules::{DetailsView, WorkspaceTab};
use crate::components::confirm::Confirm;
use crate::files::draft_store::DraftStore;

impl Root {
    pub(super) fn show_docs_tab(
        &mut self,
        _: &ShowDocsTab,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_view(DetailsView::Docs, cx);
    }

    /// Does nothing while no file is open (`pick_tab` refuses the editor tab then).
    pub(super) fn show_open_file(
        &mut self,
        _: &ShowOpenFile,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.show_tab(WorkspaceTab::Editor, cx);
    }

    /// Mod+W: closes the open file of the open workspace. The other screens stay.
    pub(super) fn close_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.open_workspace_task(cx) {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.close_open_file(&id, cx));
        }
    }

    pub(super) fn show_worktree_files(
        &mut self,
        _: &ShowWorktreeFiles,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(id) = self.open_workspace_task(cx) {
            self.dashboard
                .update(cx, |dashboard, cx| dashboard.show_files(&id, cx));
        }
    }

    /// Mod+Q and the menu: asks first while edits are not saved.
    pub(super) fn quit_app(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ask_before_quit(window, cx) {
            cx.quit();
        }
    }

    /// The window's close button. With a bench running the window quits the app instead:
    /// closing drops Root before an app quit runs its handlers, and the bench must stop first.
    pub(super) fn window_may_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.ask_before_quit(window, cx) {
            return false;
        }
        let bench_runs = matches!(self.tool, Tool::Bench(_));
        if bench_runs {
            cx.quit();
        }
        !bench_runs
    }

    /// Opens the question when there are edits to lose; true when it did.
    fn ask_before_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        self.ask_before_quit_with(window, cx, |_, cx| cx.quit())
    }

    pub(super) fn ask_before_quit_with(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        after: impl Fn(&mut Window, &mut App) + 'static,
    ) -> bool {
        let store = DraftStore::global(cx);
        let Some(question) = store.read(cx).quit_question() else {
            return false;
        };
        let rows = question
            .rows
            .iter()
            .map(|row| match row.split_once(" \u{b7} ") {
                Some((task, path)) => (task.to_owned().into(), path.to_owned().into()),
                None => (SharedString::default(), row.clone().into()),
            })
            .collect();
        let confirm = Confirm::new(
            question.title,
            "These edits are not saved. Quitting drops them.",
            "Quit",
            after,
        )
        .rows(rows);
        window.defer(cx, move |window, cx| confirm.open(window, cx));
        true
    }
}
