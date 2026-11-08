//! What the `--shot` states of screen 4 do in the Changes tab, as a user would: pick a file, step
//! to a change, select lines and press Ask agent, open Rollback.

use ui::prelude::*;

use super::ChangesView;
use crate::git::change_rules::Step;

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

    /// `path` at its second change, with the find bar open on `term` (T-0316).
    pub fn shot_find(
        &mut self,
        path: &str,
        term: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.shot_change(path, 2, window, cx);
        self.find(window, cx);
        if let Some(find) = &self.find {
            find.update(cx, |find, cx| find.set_term(term, window, cx));
        }
    }

    /// `path` shows with Annotate with Git Blame on (T-0318).
    pub fn shot_blame(&mut self, path: &str, cx: &mut Context<Self>) {
        self.pick(path, cx);
        self.blame.update(cx, |blame, cx| blame.annotate(cx));
    }

    /// Rollback… of the header: every file.
    pub fn shot_rollback(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let files = self.list.clone().unwrap_or_default();
        self.open_rollback(files, window, cx);
    }
}
