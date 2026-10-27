//! The fake's review comments (`/tasks/:id/review-comments`), with the daemon's rules: a body of
//! 1 to 5,000 characters, a draft is the only one that can be edited, resolve and reopen move a
//! sent one, and a send marks every draft sent. A pin is the stored lines: the fake reads no
//! file to move them. Each write sends `ReviewCommentsChanged`, as the daemon does.

use std::collections::BTreeMap;

use natamux_client::DashboardEvent;
use natamux_proto::workspaces::review_comments::{
    CommentSide, CommentState, MAX_COMMENT_BODY, MAX_COMMENTS, ReviewComment, SendReviewAnswer,
};
use serde_json::{Value, json};

use super::{Answer, Data, FakeDaemon, NOW, status};

/// The comments of each task, the next id number, and the ones a shot shows from the start.
#[derive(Default)]
pub(super) struct ReviewState {
    next: BTreeMap<String, u32>,
    comments: BTreeMap<String, Vec<ReviewComment>>,
    seeded: BTreeMap<String, Vec<ReviewComment>>,
}

fn number(id: &str) -> u32 {
    id.trim_start_matches('c').parse().unwrap_or(0)
}

/// By file, then first line, then id: the order of the daemon's list.
fn sorted(list: &[ReviewComment]) -> Vec<ReviewComment> {
    let mut list = list.to_vec();
    list.sort_by(|a, b| {
        (&a.path, a.start, number(&a.id)).cmp(&(&b.path, b.start, number(&b.id)))
    });
    list
}

fn checked_body(body: &str) -> Answer<String> {
    let body = body.trim();
    let length = body.chars().count();
    if length == 0 || length > MAX_COMMENT_BODY {
        return Err(status(
            400,
            format!("A comment has 1 to {MAX_COMMENT_BODY} characters."),
        ));
    }
    Ok(body.to_owned())
}

impl Data {
    pub(super) fn add_review_seed(&mut self, seed: &Value) {
        self.git.reviews.seeded = part!(seed, "comments");
    }

    fn review_changed(&mut self, task_id: &str) {
        self.send(DashboardEvent::ReviewCommentsChanged {
            task_id: task_id.into(),
        });
    }

    /// The lines of a side as text, or nothing when the fake has no text of the file.
    fn quote_of(
        &self,
        task_id: &str,
        path: &str,
        side: CommentSide,
        (start, end): (u32, u32),
    ) -> Answer<String> {
        let files = self.git.files.get(task_id);
        let file = files.and_then(|files| files.iter().find(|file| file.path == path));
        let text = file.and_then(|file| match side {
            CommentSide::Old => file.original.as_deref(),
            CommentSide::New => file.modified.as_deref(),
        });
        let Some(text) = text.filter(|_| start > 0) else {
            return Ok(String::new());
        };
        let lines: Vec<&str> = text.lines().collect();
        if end as usize > lines.len() {
            return Err(status(400, format!("{path} has {} lines.", lines.len())));
        }
        Ok(lines[start as usize - 1..end as usize].join("\n"))
    }
}

impl FakeDaemon {
    pub fn review_comments(&self, task_id: &str) -> Answer<Vec<ReviewComment>> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let list = data.git.reviews.comments.get(task_id);
        Ok(sorted(list.map_or(&[][..], Vec::as_slice)))
    }

    pub fn add_review_comment(
        &self,
        task_id: &str,
        path: &str,
        side: CommentSide,
        lines: (u32, u32),
        body: String,
    ) -> Answer<ReviewComment> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let body = checked_body(&body)?;
        let (start, end) = lines;
        if end < start || (start == 0) != (end == 0) {
            return Err(status(400, "The lines do not fit the file.".into()));
        }
        let quote = data.quote_of(task_id, path, side, lines)?;
        let reviews = &mut data.git.reviews;
        let list = reviews.comments.entry(task_id.into()).or_default();
        if list.len() >= MAX_COMMENTS {
            return Err(status(400, format!("A task has at most {MAX_COMMENTS} comments.")));
        }
        let next = reviews.next.entry(task_id.into()).or_insert(0);
        *next += 1;
        let comment = ReviewComment {
            id: format!("c{next}"),
            path: path.into(),
            side,
            start,
            end,
            quote,
            body,
            state: CommentState::Draft,
            created_at: NOW.into(),
            sent_at: None,
            resolved_at: None,
            outdated: false,
        };
        list.push(comment.clone());
        data.review_changed(task_id);
        Ok(comment)
    }

    pub fn update_review_comment(
        &self,
        task_id: &str,
        id: &str,
        body: Option<String>,
        state: Option<CommentState>,
    ) -> Answer<ReviewComment> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let body = body.map(|body| checked_body(&body)).transpose()?;
        let list = data.git.reviews.comments.entry(task_id.into()).or_default();
        let Some(comment) = list.iter_mut().find(|comment| comment.id == id) else {
            return Err(status(404, format!("{id} is not a comment of {task_id}")));
        };
        let moves = matches!(
            (state, comment.state),
            (Some(CommentState::Resolved), CommentState::Sent)
                | (Some(CommentState::Sent), CommentState::Resolved)
        );
        if (body.is_some() && comment.state != CommentState::Draft)
            || (state.is_some() && !moves)
        {
            return Err(status(409, format!("{id} is {}.", comment.state.name())));
        }
        if let Some(body) = body {
            comment.body = body;
        }
        if let Some(state) = state {
            comment.state = state;
            comment.resolved_at = (state == CommentState::Resolved).then(|| NOW.to_owned());
        }
        let done = comment.clone();
        data.review_changed(task_id);
        Ok(done)
    }

    pub fn delete_review_comment(&self, task_id: &str, id: &str) -> Answer<()> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let list = data.git.reviews.comments.entry(task_id.into()).or_default();
        let Some(at) = list.iter().position(|comment| comment.id == id) else {
            return Err(status(404, format!("{id} is not a comment of {task_id}")));
        };
        list.remove(at);
        data.review_changed(task_id);
        Ok(())
    }

    /// One paste for every draft, then they are sent. 409 with no draft or no agent.
    pub fn send_review_comments(&self, task_id: &str) -> Answer<SendReviewAnswer> {
        let mut data = self.data();
        data.task_mut(task_id)?;
        let list = data.git.reviews.comments.entry(task_id.into()).or_default();
        let drafts: Vec<String> = list
            .iter()
            .filter(|comment| comment.state == CommentState::Draft)
            .map(|comment| comment.id.clone())
            .collect();
        if drafts.is_empty() {
            return Err(status(409, "There are no draft comments to send.".into()));
        }
        if !data.agent_runs(task_id) {
            return Err(status(
                409,
                format!("No agent runs in the terminal of {task_id}. Start the agent first"),
            ));
        }
        let list = data.git.reviews.comments.entry(task_id.into()).or_default();
        for comment in list.iter_mut().filter(|c| drafts.contains(&c.id)) {
            comment.state = CommentState::Sent;
            comment.sent_at = Some(NOW.into());
        }
        let comments = sorted(list);
        let task = data.touch(task_id, "review.commented", json!({ "comments": drafts }))?;
        data.review_changed(task_id);
        Ok(SendReviewAnswer { task, comments })
    }

    /// The `changes-comments` shot: the comments of `fake_git.json` are in the task from the
    /// start. Other states keep the task without comments.
    pub fn show_review_comments(&self, task_id: &str) {
        let mut data = self.data();
        let reviews = &mut data.git.reviews;
        let Some(list) = reviews.seeded.get(task_id).cloned() else {
            return;
        };
        let next = list.iter().map(|comment| number(&comment.id)).max();
        reviews.next.insert(task_id.into(), next.unwrap_or(0));
        reviews.comments.insert(task_id.into(), list);
    }
}
