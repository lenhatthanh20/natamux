//! The git types of the Changes tab (`packages/core/src/git/changes.ts`, `commit-message.ts`): a
//! file's status, its two texts, the commit, rollback and comment calls.

use serde::{Deserialize, Serialize};

use crate::tasks::task::Task;

/// A text file larger than this many bytes comes back with `tooLarge` and no text.
pub const MAX_FILE_BYTES: u64 = 1_000_000;
/// An image larger than this many bytes is refused (413).
pub const MAX_IMAGE_BYTES: u64 = 5_000_000;
/// `COMMIT_SUBJECT_MAX`: the first line of a drafted message is cut at this many characters.
pub const COMMIT_SUBJECT_MAX: usize = 72;
/// A commit message: at most this many characters, after trimming.
pub const MAX_COMMIT_MESSAGE: usize = 10_000;
/// A rollback: at most this many paths, each at most `MAX_PATH_LENGTH` characters.
pub const MAX_ROLLBACK_PATHS: usize = 5_000;
pub const MAX_PATH_LENGTH: usize = 1_024;
/// A message for the agent: at most this many characters, after trimming.
pub const MAX_AGENT_MESSAGE: usize = 20_000;
/// `MAX_GRAPH_COMMITS` of the daemon: the graph lists at most this many commits.
pub const MAX_GRAPH_COMMITS: usize = 200;

/// `IMAGE_TYPES`: the extensions the image route serves, with their MIME types.
pub const IMAGE_TYPES: [(&str, &str); 6] = [
    ("png", "image/png"),
    ("jpg", "image/jpeg"),
    ("jpeg", "image/jpeg"),
    ("gif", "image/gif"),
    ("webp", "image/webp"),
    ("svg", "image/svg+xml"),
];

/// How a file differs from the last commit (`changeStatusSchema`). An unknown letter of a later
/// daemon reads as Modified, so the file still shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum ChangeStatus {
    Modified,
    Added,
    Deleted,
    Renamed,
    /// Not tracked by git yet.
    Untracked,
    Conflict,
}

impl ChangeStatus {
    pub const ALL: [ChangeStatus; 6] = [
        Self::Modified,
        Self::Added,
        Self::Deleted,
        Self::Renamed,
        Self::Untracked,
        Self::Conflict,
    ];

    /// The letter git and the daemon use.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "U",
            Self::Conflict => "C",
        }
    }

    /// `CHANGE_LABEL` of `lib/changes.ts`.
    pub fn label(self) -> &'static str {
        match self {
            Self::Modified => "Modified",
            Self::Added => "Added",
            Self::Deleted => "Deleted",
            Self::Renamed => "Renamed",
            Self::Untracked => "New file",
            Self::Conflict => "Conflict",
        }
    }

    pub fn parse(letter: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|status| status.letter() == letter)
            .unwrap_or(Self::Modified)
    }
}

impl From<String> for ChangeStatus {
    fn from(letter: String) -> Self {
        Self::parse(&letter)
    }
}

impl From<&str> for ChangeStatus {
    fn from(letter: &str) -> Self {
        Self::parse(letter)
    }
}

impl From<ChangeStatus> for String {
    fn from(status: ChangeStatus) -> Self {
        status.letter().to_string()
    }
}

/// The MIME type of an image path by its extension, any case; None when it is no image.
pub fn image_type_of(path: &str) -> Option<&'static str> {
    let name = path.rsplit('/').next().unwrap_or(path);
    let (_, ext) = name.rsplit_once('.')?;
    let ext = ext.to_ascii_lowercase();
    IMAGE_TYPES
        .iter()
        .find(|(known, _)| *known == ext)
        .map(|(_, mime)| *mime)
}

/// Which side of a changed image to read: the last commit or the working tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ImageSide {
    Original,
    Modified,
}

impl ImageSide {
    pub fn name(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Modified => "modified",
        }
    }
}

/// The body of `GET /tasks/:id/changes/file` (`fileDiffSchema`): the text at the last commit and
/// in the working tree. A side is None when the file is not there, or is binary or too large.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileDiff {
    pub path: String,
    pub status: ChangeStatus,
    #[serde(default)]
    pub original: Option<String>,
    #[serde(default)]
    pub modified: Option<String>,
    #[serde(default)]
    pub binary: bool,
    #[serde(default)]
    pub too_large: bool,
}

/// The body of `POST /tasks/:id/commit`: the message, trimmed, 1 to `MAX_COMMIT_MESSAGE`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct CommitInput {
    pub message: String,
}

/// The answer of `POST /tasks/:id/commit`.
#[derive(Clone, Debug, Deserialize)]
pub struct CommitAnswer {
    pub task: Task,
    #[serde(default)]
    pub hash: String,
}

/// The body of `POST /tasks/:id/rollback`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct RollbackInput {
    pub paths: Vec<String>,
}

/// The answer of a rollback and of its undo (`rollbackResultSchema`): how many files changed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct RollbackResult {
    #[serde(default)]
    pub files: u64,
}

/// Where a drafted commit message came from: the task's agent, or the daemon's simple draft
/// when the agent could not write one.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DraftSource {
    Agent,
    #[serde(other)]
    Draft,
}

/// The answer of `POST /tasks/:id/commit-message` (`commitMessageDraftSchema`).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct CommitMessageDraft {
    pub message: String,
    pub source: DraftSource,
    #[serde(default)]
    pub agent: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_every_status_letter_and_an_unknown_one_as_modified() {
        for status in ChangeStatus::ALL {
            let read: ChangeStatus =
                serde_json::from_str(&format!("\"{}\"", status.letter())).unwrap();
            assert_eq!(read, status);
        }
        let later: ChangeStatus = serde_json::from_str("\"T\"").unwrap();
        assert_eq!(later, ChangeStatus::Modified);
        assert_eq!(
            serde_json::to_string(&ChangeStatus::Untracked).unwrap(),
            "\"U\""
        );
        assert_eq!(ChangeStatus::Untracked.label(), "New file");
    }

    #[test]
    fn finds_an_image_by_its_extension_in_any_case() {
        assert_eq!(image_type_of("public/banner.PNG"), Some("image/png"));
        assert_eq!(image_type_of("a/b.jpeg"), Some("image/jpeg"));
        assert_eq!(image_type_of("icon.svg"), Some("image/svg+xml"));
        assert_eq!(image_type_of("src/a.ts"), None);
        assert_eq!(image_type_of("png"), None);
        assert_eq!(image_type_of(".config/png"), None);
    }

    #[test]
    fn reads_a_file_diff_with_and_without_its_sides() {
        let diff: FileDiff = serde_json::from_str(
            r#"{"path":"a.ts","status":"A","original":null,"modified":"x","binary":false,"tooLarge":false}"#,
        )
        .unwrap();
        assert_eq!(diff.status, ChangeStatus::Added);
        assert_eq!(diff.original, None);
        assert_eq!(diff.modified.as_deref(), Some("x"));
        let big: FileDiff =
            serde_json::from_str(r#"{"path":"b","status":"M","tooLarge":true}"#).unwrap();
        assert!(big.too_large);
    }

    #[test]
    fn sends_a_commit_and_a_rollback_and_reads_their_answers() {
        let commit = CommitInput {
            message: "Fix it".into(),
        };
        assert_eq!(
            serde_json::to_string(&commit).unwrap(),
            r#"{"message":"Fix it"}"#
        );
        let rollback = RollbackInput {
            paths: vec!["a".into(), "b/c".into()],
        };
        assert_eq!(
            serde_json::to_string(&rollback).unwrap(),
            r#"{"paths":["a","b/c"]}"#
        );
        let result: RollbackResult = serde_json::from_str(r#"{"files":2}"#).unwrap();
        assert_eq!(result.files, 2);
    }

    #[test]
    fn reads_a_draft_with_and_without_a_reason() {
        let agent: CommitMessageDraft =
            serde_json::from_str(r#"{"message":"m","source":"agent","agent":"claude-code"}"#)
                .unwrap();
        assert_eq!(agent.source, DraftSource::Agent);
        assert_eq!(agent.reason, None);
        let draft: CommitMessageDraft = serde_json::from_str(
            r#"{"message":"m","source":"draft","agent":"codex","reason":"timed out"}"#,
        )
        .unwrap();
        assert_eq!(draft.source, DraftSource::Draft);
        assert_eq!(draft.reason.as_deref(), Some("timed out"));
    }
}
