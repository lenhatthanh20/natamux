//! The git calls of the Changes and History tabs (screen 4), from the daemon or the fake.

use natamux_client::Answer;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::changes::{
    CommitAnswer, CommitMessageDraft, FileDiff, ImageSide, RollbackResult,
};
use natamux_proto::workspaces::review_comments::{
    CommentSide, CommentState, ReviewComment, SendReviewAnswer,
};
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};

use super::Source;

impl Source {
    /// The files not committed; `fresh` skips the daemon's 5 s cache (Refresh).
    pub fn read_changes(&self, task_id: &str, fresh: bool) -> Answer<Vec<ChangeEntry>> {
        self.ask(
            |c| c.list_changes(task_id, fresh),
            |f| f.list_changes(task_id, fresh),
        )
    }

    pub fn read_graph(&self, task_id: &str, fresh: bool) -> Answer<GitGraph> {
        self.ask(|c| c.graph(task_id, fresh), |f| f.graph(task_id, fresh))
    }

    pub fn file_diff(&self, task_id: &str, path: &str) -> Answer<FileDiff> {
        self.ask(
            |c| c.file_diff(task_id, path),
            |f| f.file_diff(task_id, path),
        )
    }

    pub fn file_image(&self, task_id: &str, path: &str, side: ImageSide) -> Answer<Vec<u8>> {
        self.ask(
            |c| c.file_image(task_id, path, side),
            |f| f.file_image(task_id, path, side),
        )
    }

    pub fn commit(&self, task_id: &str, message: String) -> Answer<CommitAnswer> {
        let fake = message.clone();
        self.ask(|c| c.commit(task_id, message), |f| f.commit(task_id, fake))
    }

    pub fn commit_message(&self, task_id: &str) -> Answer<CommitMessageDraft> {
        self.ask(|c| c.commit_message(task_id), |f| f.commit_message(task_id))
    }

    pub fn rollback(&self, task_id: &str, paths: Vec<String>) -> Answer<RollbackResult> {
        let fake = paths.clone();
        self.ask(
            |c| c.rollback(task_id, paths),
            |f| f.rollback(task_id, fake),
        )
    }

    pub fn undo_rollback(&self, task_id: &str) -> Answer<RollbackResult> {
        self.ask(|c| c.undo_rollback(task_id), |f| f.undo_rollback(task_id))
    }

    pub fn send_to_agent(&self, task_id: &str, text: String) -> Answer<Task> {
        let fake = text.clone();
        self.ask(
            |c| c.send_to_agent(task_id, text),
            |f| f.send_to_agent(task_id, fake),
        )
    }

    pub fn review_comments(&self, task_id: &str) -> Answer<Vec<ReviewComment>> {
        self.ask(
            |c| c.review_comments(task_id),
            |f| f.review_comments(task_id),
        )
    }

    pub fn add_review_comment(
        &self,
        task_id: &str,
        path: &str,
        side: CommentSide,
        lines: (u32, u32),
        body: String,
    ) -> Answer<ReviewComment> {
        let fake = body.clone();
        self.ask(
            |c| c.add_review_comment(task_id, path, side, lines, body),
            |f| f.add_review_comment(task_id, path, side, lines, fake),
        )
    }

    pub fn update_review_comment(
        &self,
        task_id: &str,
        id: &str,
        body: Option<String>,
        state: Option<CommentState>,
    ) -> Answer<ReviewComment> {
        let fake = body.clone();
        self.ask(
            |c| c.update_review_comment(task_id, id, body, state),
            |f| f.update_review_comment(task_id, id, fake, state),
        )
    }

    pub fn delete_review_comment(&self, task_id: &str, id: &str) -> Answer<()> {
        self.ask(
            |c| c.delete_review_comment(task_id, id),
            |f| f.delete_review_comment(task_id, id),
        )
    }

    pub fn send_review_comments(&self, task_id: &str) -> Answer<SendReviewAnswer> {
        self.ask(
            |c| c.send_review_comments(task_id),
            |f| f.send_review_comments(task_id),
        )
    }
}
