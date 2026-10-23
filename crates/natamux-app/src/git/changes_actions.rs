//! The keys of the Changes tab (`lib/commands`): Refresh (Shift+R), the previous and next file
//! (`[`, `]`), the previous and next change (Shift+Alt+F5, Alt+F5), comment or Ask agent
//! (Mod+Shift+A), roll back the shown file (Mod+Alt+Z) and draft the commit message
//! (Mod+Shift+G in the commit box).

use ui::prelude::*;

use super::ChangesView;
use crate::app::shortcuts::{
    CommentOnDiff, DraftCommit, NextChange, NextFile, PrevChange, PrevFile, RefreshChanges,
    RollbackFile,
};
use crate::git::change_rules::{Step, step_file};

impl ChangesView {
    /// Refresh: the list past the daemon's cache, and the shown file again.
    pub(super) fn refresh(&mut self, cx: &mut Context<Self>) {
        self.workspace
            .update(cx, |workspace, cx| workspace.read_git_fresh(cx))
            .ok();
    }

    /// `[` and `]`: files in the panel's order; they stop at the ends.
    pub(super) fn step_file(&mut self, step: Step, cx: &mut Context<Self>) {
        let order = self.order(cx);
        let current = self.shown_path(cx);
        if let Some(path) = step_file(&order, current.as_deref(), step).map(str::to_owned) {
            self.pick(&path, cx);
        }
    }

    fn rollback_shown(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entry) = self.shown_entry(cx) {
            self.open_rollback(vec![entry], window, cx);
        }
    }

    pub(super) fn with_actions(&self, root: Div, cx: &mut Context<Self>) -> Div {
        root.on_action(cx.listener(|view, _: &RefreshChanges, _, cx| view.refresh(cx)))
            .on_action(cx.listener(|view, _: &PrevFile, _, cx| view.step_file(Step::Prev, cx)))
            .on_action(cx.listener(|view, _: &NextFile, _, cx| view.step_file(Step::Next, cx)))
            .on_action(cx.listener(|view, _: &NextChange, window, cx| {
                view.diff
                    .update(cx, |diff, cx| diff.step_change(Step::Next, window, cx))
            }))
            .on_action(cx.listener(|view, _: &PrevChange, window, cx| {
                view.diff
                    .update(cx, |diff, cx| diff.step_change(Step::Prev, window, cx))
            }))
            .on_action(
                cx.listener(|view, _: &CommentOnDiff, window, cx| view.comment_on_diff(window, cx)),
            )
            .on_action(
                cx.listener(|view, _: &RollbackFile, window, cx| view.rollback_shown(window, cx)),
            )
            .on_action(cx.listener(|view, _: &DraftCommit, window, cx| view.draft(window, cx)))
    }
}
