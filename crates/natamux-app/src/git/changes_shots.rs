//! What the `--shot` states of screen 4 do in the Changes tab, as a user would: pick a file, step
//! to a change, select lines and press Ask agent, open Rollback.

use ui::prelude::*;

use super::ChangesView;
use crate::git::change_rules::Step;
use crate::git::rollback_dialog;

impl ChangesView {
    /// `path` shows, and the cursor goes to its change number `change` (1-based).
    pub fn shot_change(
        &mut self,
        path: &str,
        change: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.pick(path, cx);
        for _ in 0..change {
            self.diff
                .update(cx, |diff, cx| diff.step_change(Step::Next, window, cx));
        }
        self.diff.update(cx, |diff, cx| diff.focus(window, cx));
    }

    /// `path` shows with its language parsed (`DiffView::parsed`).
    pub fn shot_parsed(&self, path: &str, cx: &App) -> bool {
        self.diff.read(cx).parsed(path, cx)
    }

    /// `shot_parsed`, and the last commit's side of the diff parsed too.
    pub fn shot_base_parsed(&self, path: &str, cx: &App) -> bool {
        let diff = self.diff.read(cx);
        diff.parsed(path, cx) && diff.base_parsed(cx)
    }

    /// Lines `first` to `last` of the working tree picked, then Ask agent.
    pub fn shot_ask(&mut self, first: u32, last: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.update(cx, |diff, cx| {
            diff.focus(window, cx);
            diff.select_lines(first, last, window, cx);
        });
        self.comment_on_diff(window, cx);
    }

    /// `path` shows with its review comments (the fake has them from the start).
    pub fn shot_comments(&mut self, path: &str, cx: &mut Context<Self>) {
        self.pick(path, cx);
        self.read_comments(cx);
    }

    /// Rollback… of the header: every file.
    pub fn shot_rollback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let files = self.list.clone().unwrap_or_default();
        self.open_rollback(files, window, cx);
    }

    /// `path` picked in the list; its diff is read and drawn after.
    pub fn shot_pick(&mut self, path: &str, cx: &mut Context<Self>) {
        self.pick(path, cx);
    }

    /// `path` shows, with no wait for a language: for a file that has none.
    pub fn shot_shown(&self, path: &str, cx: &App) -> bool {
        self.diff.read(cx).shown(path)
    }

    /// The change the cursor is in, 0-based, as the toolbar's "2 of 7" reads it.
    pub fn shot_change_index(&mut self, cx: &mut Context<Self>) -> Option<usize> {
        self.diff.update(cx, |diff, cx| diff.change_index(cx))
    }

    /// The diff focused, with lines `first` to `last` of the working tree picked.
    pub fn shot_select(
        &mut self,
        first: u32,
        last: u32,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.diff.update(cx, |diff, cx| {
            diff.focus(window, cx);
            diff.select_lines(first, last, window, cx);
        });
    }

    /// Rollback… of every file, with `unchecked` unchecked: a box on, one off, and the box of a
    /// conflict, which is off and disabled.
    pub fn shot_checkboxes(
        &mut self,
        unchecked: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let files = self.list.clone().unwrap_or_default();
        let dialog = rollback_dialog::show(self.workspace.clone(), files, window, cx);
        dialog.update(cx, |dialog, cx| dialog.uncheck(unchecked, cx));
    }
}
