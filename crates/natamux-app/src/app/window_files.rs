//! The keys and menu items of screen 6 that act on the open workspace: the Docs tab, the open
//! file, the files of the worktree. And quitting, which always asks first: a slipped Mod+Q would
//! end every agent. With edits not saved it names them, as the browser does before it closes a
//! page with a draft (`useWarnOnUnsavedFiles`).

use ui::prelude::*;

use super::{Root, Tool};
use crate::app::shortcuts::{CloseTab, Quit, ShowDocsTab, ShowOpenFile, ShowWorktreeFiles};
use crate::app::view_rules::{DetailsView, WorkspaceTab};
use crate::components::confirm::Confirm;
use crate::files::draft_store::DraftStore;
use crate::files::file_drafts::QuitQuestion;

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

    /// Mod+Q, the menu and the palette: asks first.
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

    /// Opens the question; true when it did. A tool run (bench, IME check) has no person to
    /// ask, so it asks only when there are edits to lose.
    fn ask_before_quit(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let plain = matches!(self.tool, Tool::None);
        self.ask_before_quit_with(plain, window, cx, |_, cx| cx.quit())
    }

    /// `plain`: ask also when no edits would be lost.
    pub(super) fn ask_before_quit_with(
        &mut self,
        plain: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
        after: impl Fn(&mut Window, &mut App) + 'static,
    ) -> bool {
        let drafts = DraftStore::global(cx).read(cx).quit_question();
        let Some(text) = quit_text(drafts, plain) else {
            return false;
        };
        let confirm = Confirm::new(text.title, text.description, "Quit", after).rows(text.rows);
        window.defer(cx, move |window, cx| confirm.open(window, cx));
        true
    }
}

/// What the quit question says.
#[derive(Debug, PartialEq)]
struct QuitText {
    title: SharedString,
    description: SharedString,
    /// (task, path) of each draft.
    rows: Vec<(SharedString, SharedString)>,
}

const PLAIN_QUIT: &str =
    "Every agent and terminal stops. They come back the next time you open Natamux.";

/// None when quitting needs no question.
fn quit_text(drafts: Option<QuitQuestion>, plain: bool) -> Option<QuitText> {
    let Some(question) = drafts else {
        return plain.then(|| QuitText {
            title: "Quit Natamux?".into(),
            description: PLAIN_QUIT.into(),
            rows: Vec::new(),
        });
    };
    let rows = question
        .rows
        .iter()
        .map(|row| match row.split_once(" \u{b7} ") {
            Some((task, path)) => (task.to_owned().into(), path.to_owned().into()),
            None => (SharedString::default(), row.clone().into()),
        })
        .collect();
    Some(QuitText {
        title: question.title.into(),
        description: "These edits are not saved. Quitting drops them.".into(),
        rows,
    })
}

#[cfg(test)]
#[path = "window_files_tests.rs"]
mod tests;
