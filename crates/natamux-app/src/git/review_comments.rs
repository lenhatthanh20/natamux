//! The review comments of the shown task in the Changes tab: read with the diff and on the
//! daemon's events, added from the comment box (Mod+Enter), edited while they are drafts,
//! deleted, resolved and reopened, and sent to the agent in one batch (Mod+Alt+Enter). Pure
//! rules are in `review_rules.rs`; the blocks in the diff are in `review_blocks.rs`.

use gpui::{Focusable, Task};
use natamux_proto::workspaces::review_comments::{CommentSide, CommentState, ReviewComment};
use ui::prelude::*;

use super::ChangesView;
use crate::app::dashboard::Written;
use crate::app::source::reason;
use crate::app::view_rules::WorkspaceTab;
use crate::git::change_rules::Side;
use crate::git::comment_rules::send_reason;
use crate::git::review_rules::{CommentAction, draft_count, is_visible, target};

/// The comments of the shown task and what the user is doing with them.
#[derive(Default)]
pub(super) struct Review {
    pub(super) comments: Vec<ReviewComment>,
    /// Per view, not saved.
    pub(super) show_resolved: bool,
    /// The draft that the comment box edits.
    pub(super) editing: Option<String>,
    /// A failed send, delete or resolve; the list shows it under Send.
    pub(super) error: Option<String>,
    pub(super) sending: bool,
    /// A list click waits for the file's editors: its path, side and line.
    jump: Option<(String, Side, u32)>,
    _read: Option<Task<()>>,
}

impl ChangesView {
    /// Another task starts with no comments.
    pub(super) fn reset_review(&mut self) {
        self.review = Review {
            show_resolved: self.review.show_resolved,
            ..Review::default()
        };
    }

    /// The task's comments, pinned by the daemon. A failed read keeps what is shown.
    pub fn read_comments(&mut self, cx: &mut Context<Self>) {
        let (Some(id), Some(source)) = (
            self.task_id.clone(),
            self.dashboard.read(cx).source().cloned(),
        ) else {
            return;
        };
        let answer = source.review_comments(&id);
        self.review._read = Some(cx.spawn(async move |this, cx| {
            let read = answer.await;
            this.update(cx, |this, cx| {
                if this.task_id.as_deref() != Some(id.as_str()) {
                    return;
                }
                let Ok(comments) = read else {
                    return;
                };
                let editable = this.review.editing.as_ref().is_some_and(|editing| {
                    comments
                        .iter()
                        .any(|c| &c.id == editing && c.state == CommentState::Draft)
                });
                if !editable {
                    this.review.editing = None;
                }
                if this.review.comments != comments {
                    this.review.comments = comments;
                }
                cx.notify();
            })
            .ok();
        }));
    }

    /// The comments the diff draws: resolved ones only when asked for.
    pub(super) fn shown_comments(&self) -> Vec<ReviewComment> {
        let show = self.review.show_resolved;
        let visible = self.review.comments.iter().filter(|c| is_visible(c, show));
        visible.cloned().collect()
    }

    pub(super) fn toggle_resolved(&mut self, cx: &mut Context<Self>) {
        self.review.show_resolved = !self.review.show_resolved;
        cx.notify();
    }

    /// Add comment (Mod+Enter in the box), or Save comment while a draft is edited. The box and
    /// the quote clear only when it worked.
    pub(super) fn add_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(id), Some(path)) = (self.task_id.clone(), self.shown_path(cx)) else {
            return;
        };
        let comment = self.comment_box(window, cx);
        let text = comment.read(cx).text(cx);
        if self.sending || send_reason(&text).is_some() {
            return;
        }
        let line = self.diff.update(cx, |diff, cx| diff.cursor_line(cx));
        let aim = target(self.quote.as_ref(), line);
        let editing = self.review.editing.clone();
        self.sending = true;
        self.sent = None;
        let answer = self.dashboard.update(cx, |dashboard, cx| match editing {
            Some(edited) => dashboard.update_review_comment(&id, &edited, Some(text), None, cx),
            None => {
                let lines = (aim.start, aim.end);
                dashboard.add_review_comment(&id, &path, aim.side, lines, text, cx)
            }
        });
        cx.spawn_in(window, async move |view, cx| {
            let result = answer.await;
            view.update_in(cx, |view, window, cx| {
                view.sending = false;
                match result {
                    Ok(_) => {
                        view.quote = None;
                        view.review.editing = None;
                        comment.update(cx, |editor, cx| editor.set_text("", window, cx));
                        view.read_comments(cx);
                    }
                    Err(error) => view.sent = Some(Err(reason(&error))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Edit on a draft's block: its text goes into the box, and Save comment writes it back.
    fn edit_comment(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(body) = self
            .review
            .comments
            .iter()
            .find(|c| c.id == id && c.state == CommentState::Draft)
            .map(|c| c.body.clone())
        else {
            return;
        };
        self.review.editing = Some(id.to_owned());
        self.quote = None;
        self.sent = None;
        let comment = self.comment_box(window, cx);
        comment.update(cx, |editor, cx| editor.set_text(body, window, cx));
        window.focus(&comment.focus_handle(cx), cx);
        cx.notify();
    }

    pub(super) fn cancel_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.review.editing = None;
        self.sent = None;
        let comment = self.comment_box(window, cx);
        comment.update(cx, |editor, cx| editor.set_text("", window, cx));
        cx.notify();
    }

    /// A button of a block: the block's view calls this.
    pub fn run_comment_action(
        &mut self,
        action: CommentAction,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(task_id) = self.task_id.clone() else {
            return;
        };
        match action {
            CommentAction::Edit => self.edit_comment(id, window, cx),
            CommentAction::Delete => {
                if self.review.editing.as_deref() == Some(id) {
                    self.cancel_edit(window, cx);
                }
                let answer = self.dashboard.update(cx, |dashboard, cx| {
                    dashboard.delete_review_comment(&task_id, id, cx)
                });
                self.finish_review_write(answer, cx);
            }
            CommentAction::Resolve => {
                self.set_comment_state(&task_id, id, CommentState::Resolved, cx)
            }
            CommentAction::Reopen => self.set_comment_state(&task_id, id, CommentState::Sent, cx),
        }
    }

    fn set_comment_state(
        &mut self,
        task_id: &str,
        id: &str,
        state: CommentState,
        cx: &mut Context<Self>,
    ) {
        let answer = self.dashboard.update(cx, |dashboard, cx| {
            dashboard.update_review_comment(task_id, id, None, Some(state), cx)
        });
        self.finish_review_write(answer, cx);
    }

    /// After a write: read the list again, or show why it failed.
    fn finish_review_write<T: 'static>(&mut self, answer: Written<T>, cx: &mut Context<Self>) {
        self.review.error = None;
        cx.spawn(async move |view, cx| {
            let result = answer.await;
            view.update(cx, |view, cx| {
                match result {
                    Ok(_) => view.read_comments(cx),
                    Err(error) => view.review.error = Some(reason(&error)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Send N comments (Mod+Alt+Enter): every draft goes to the agent in one prompt.
    pub(super) fn send_comments(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.task_id.clone() else {
            return;
        };
        if self.review.sending || draft_count(&self.review.comments) == 0 {
            return;
        }
        self.review.sending = true;
        self.review.error = None;
        let answer = self
            .dashboard
            .update(cx, |dashboard, cx| dashboard.send_review_comments(&id, cx));
        cx.spawn(async move |view, cx| {
            let result = answer.await;
            view.update(cx, |view, cx| {
                view.review.sending = false;
                match result {
                    Ok(sent) => view.review.comments = sent.comments,
                    Err(error) => view.review.error = Some(reason(&error)),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// A click on a row of the list: its file shows and the cursor goes to its first line.
    pub(super) fn go_to_comment(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(comment) = self.review.comments.iter().find(|c| c.id == id).cloned() else {
            return;
        };
        self.pick(&comment.path, cx);
        self.dashboard.update(cx, |dashboard, cx| {
            dashboard.pick_tab(WorkspaceTab::Changes, cx)
        });
        let side = match comment.side {
            CommentSide::Old => Side::Old,
            CommentSide::New => Side::New,
        };
        self.review.jump = Some((comment.path, side, comment.start.max(1)));
        cx.notify();
    }

    /// Runs a click's jump once the editors show its file.
    pub(super) fn take_jump(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((path, side, line)) = self.review.jump.clone() else {
            return;
        };
        if self.shown_path(cx).as_deref() != Some(path.as_str()) {
            self.review.jump = None;
            return;
        }
        if !self.diff.read(cx).shows(&path) {
            return;
        }
        self.review.jump = None;
        cx.defer_in(window, move |view, window, cx| {
            view.diff.update(cx, |diff, cx| {
                diff.go_to_line(side, line, window, cx);
                diff.focus(window, cx);
            });
        });
    }
}
