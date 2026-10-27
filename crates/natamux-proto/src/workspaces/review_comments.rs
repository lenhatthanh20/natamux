//! Review comments on a task's diff (`/tasks/:id/review-comments`,
//! `packages/core/src/git/review-comments.ts`): notes a person leaves on lines of one side, kept
//! by the daemon and sent to the agent in one batch.

use serde::{Deserialize, Serialize};

use crate::tasks::task::Task;

/// A comment's text: at most this many characters, after trimming.
pub const MAX_COMMENT_BODY: usize = 5_000;
/// A task keeps at most this many comments, in any state.
pub const MAX_COMMENTS: usize = 500;
/// The file in the task's stage folder that a send writes for the agent.
pub const REVIEW_COMMENTS_FILE: &str = "review-comments.md";

/// `old` is the text of the last commit, `new` the working tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentSide {
    Old,
    New,
}

impl CommentSide {
    pub fn name(self) -> &'static str {
        match self {
            Self::Old => "old",
            Self::New => "new",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "old" => Some(Self::Old),
            "new" => Some(Self::New),
            _ => None,
        }
    }
}

/// A draft waits for the next send; a sent one waits for the person to resolve it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CommentState {
    Draft,
    Sent,
    Resolved,
}

impl CommentState {
    pub fn name(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Sent => "sent",
            Self::Resolved => "resolved",
        }
    }

    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "draft" => Some(Self::Draft),
            "sent" => Some(Self::Sent),
            "resolved" => Some(Self::Resolved),
            _ => None,
        }
    }
}

/// One comment. `start` and `end` are 1-based lines of `side`; both 0 for the whole file, with an
/// empty `quote`. `outdated` is worked out on each list and never stored.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewComment {
    pub id: String,
    pub path: String,
    pub side: CommentSide,
    pub start: u32,
    pub end: u32,
    pub quote: String,
    pub body: String,
    pub state: CommentState,
    pub created_at: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub outdated: bool,
}

fn is_false(value: &bool) -> bool {
    !*value
}

impl ReviewComment {
    /// Draft or sent: still for the agent.
    pub fn open(&self) -> bool {
        self.state != CommentState::Resolved
    }
}

/// `<data>/tasks/<ID>/review-comments.json`. `next` numbers the next id, so a deleted id is never
/// used again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct ReviewCommentsFile {
    #[serde(default)]
    pub next: u32,
    #[serde(default)]
    pub comments: Vec<ReviewComment>,
}

/// The body of `POST /tasks/:id/review-comments`. The daemon reads the quote itself.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct NewReviewComment {
    pub path: String,
    pub side: CommentSide,
    pub start: u32,
    pub end: u32,
    pub body: String,
}

/// The body of `PATCH /tasks/:id/review-comments/:cid`.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct ReviewCommentPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<CommentState>,
}

/// The answer of `GET /tasks/:id/review-comments`.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReviewCommentList {
    pub comments: Vec<ReviewComment>,
}

/// The answer of `POST` and `PATCH` of one comment.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ReviewCommentAnswer {
    pub comment: ReviewComment,
}

/// The answer of `POST /tasks/:id/review-comments/send`: the task and every comment after it.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SendReviewAnswer {
    pub task: Task,
    pub comments: Vec<ReviewComment>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_comment_has_no_outdated_field_and_reads_back() {
        let comment = ReviewComment {
            id: "c1".into(),
            path: "src/a.ts".into(),
            side: CommentSide::New,
            start: 2,
            end: 3,
            quote: "a\nb".into(),
            body: "Rename".into(),
            state: CommentState::Draft,
            created_at: "2026-10-27T00:00:00.000Z".into(),
            sent_at: None,
            resolved_at: None,
            outdated: false,
        };
        let text = serde_json::to_string(&comment).unwrap();
        assert!(!text.contains("outdated"), "{text}");
        assert!(!text.contains("sentAt"), "{text}");
        assert!(text.contains("\"createdAt\""), "{text}");
        assert!(text.contains("\"side\":\"new\""), "{text}");
        assert_eq!(serde_json::from_str::<ReviewComment>(&text).unwrap(), comment);
    }

    #[test]
    fn an_empty_file_reads_as_no_comments() {
        let file: ReviewCommentsFile = serde_json::from_str("{}").unwrap();
        assert_eq!(file, ReviewCommentsFile::default());
    }

    #[test]
    fn names_parse_back() {
        for side in [CommentSide::Old, CommentSide::New] {
            assert_eq!(CommentSide::parse(side.name()), Some(side));
        }
        for state in [CommentState::Draft, CommentState::Sent, CommentState::Resolved] {
            assert_eq!(CommentState::parse(state.name()), Some(state));
        }
        assert_eq!(CommentState::parse("open"), None);
    }
}
