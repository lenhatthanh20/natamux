//! The errors of the workspace routes, with the Node daemon's texts (`workspace-errors.ts`,
//! `workspace-files.ts`) and its HTTP mapping (`http/errors.ts`).

use natamux_git::commit_push::{CommitError, NOTHING_TO_COMMIT};
use natamux_git::exec::{GitError, OutsideWorkspaceError};
use natamux_git::file_diff::{FileDiffError, FileImageError, MAX_FILE_BYTES};
use natamux_proto::tasks::task::Task;

use crate::tasks::errors::TaskError;

#[derive(Debug)]
pub enum WorkspaceError {
    /// `WorkspaceConflictError`: 409 `conflict`.
    Conflict(String),
    /// `FileNotChangedError` with the path: 404 `not_found`.
    NotChanged(String),
    /// `WorkspacePathNotFoundError` with the path: 404 `not_found`.
    PathNotFound(String),
    /// `WorkspacePathError`: the path is not a `file` or `folder`: 400 `invalid_request`.
    NotA {
        path: String,
        kind: &'static str,
    },
    /// `GitFolderError` with the path: 409 `conflict`.
    GitFolder(String),
    /// `FileChangedOnDiskError` with the path: 409 `conflict`.
    ChangedOnDisk(String),
    /// `FileTooLargeError` with the path: 413 `too_large`.
    TooLarge(String),
    Git(GitError),
    Outside(OutsideWorkspaceError),
    /// `NothingToCommitError`: 409 `conflict`.
    NothingToCommit,
    /// `GitHubError`, the message of the failed `gh` run: 409 `conflict`.
    GitHub(String),
    /// A changed image that cannot be sent: 400, 404, 413 or 409 by its kind.
    Image(FileImageError),
    /// A changed file that cannot be read: 409 for git, 500 for a failed read.
    Diff(FileDiffError),
    /// A file that exists but cannot be read or written (raw io error): 500 `internal_error`.
    Io(String),
    /// A request the route refuses, such as `If-Match is missing`: 400 `invalid_request`.
    BadRequest(String),
    Task(TaskError),
}

impl std::fmt::Display for WorkspaceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Conflict(text) | Self::GitHub(text) | Self::Io(text) | Self::BadRequest(text) => {
                f.write_str(text)
            }
            Self::NotChanged(path) => {
                write!(f, "{path} is not one of the changed files of the workspace")
            }
            Self::PathNotFound(path) => write!(f, "{path} is not in the workspace"),
            Self::NotA { path, kind } => {
                let name = if path.is_empty() {
                    "The workspace"
                } else {
                    path
                };
                write!(f, "{name} is not a {kind}")
            }
            Self::GitFolder(path) => write!(f, "{path} is inside .git, so it is not shown"),
            Self::ChangedOnDisk(path) => write!(f, "{path} changed on disk after you opened it"),
            Self::TooLarge(path) => write!(
                f,
                "{path} would be larger than {} MB, so it is not saved",
                MAX_FILE_BYTES / 1_000_000
            ),
            Self::Git(error) => std::fmt::Display::fmt(error, f),
            Self::Outside(error) => std::fmt::Display::fmt(error, f),
            Self::NothingToCommit => f.write_str(NOTHING_TO_COMMIT),
            Self::Image(error) => std::fmt::Display::fmt(error, f),
            Self::Diff(error) => std::fmt::Display::fmt(error, f),
            Self::Task(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for WorkspaceError {}

impl WorkspaceError {
    /// The HTTP status and the `error` code Node answers with. A `Task(Invalid(..))` is 400
    /// `invalid_request`; the route sends zod's issues itself.
    pub fn status_and_code(&self) -> (u16, &'static str) {
        match self {
            Self::NotChanged(_) | Self::PathNotFound(_) => (404, "not_found"),
            Self::NotA { .. } | Self::BadRequest(_) => (400, "invalid_request"),
            Self::TooLarge(_) => (413, "too_large"),
            Self::Conflict(_)
            | Self::GitFolder(_)
            | Self::ChangedOnDisk(_)
            | Self::Git(_)
            | Self::Outside(_)
            | Self::NothingToCommit
            | Self::GitHub(_) => (409, "conflict"),
            Self::Io(_) => (500, "internal_error"),
            Self::Image(error) => match error {
                FileImageError::NotAnImage(_) => (400, "invalid_request"),
                FileImageError::SideMissing(..) => (404, "not_found"),
                FileImageError::TooLarge(_) => (413, "too_large"),
                FileImageError::Git(_) | FileImageError::Outside(_) => (409, "conflict"),
                FileImageError::Read(_) => (500, "internal_error"),
            },
            Self::Diff(error) => match error {
                FileDiffError::Git(_) | FileDiffError::Outside(_) => (409, "conflict"),
                FileDiffError::Read(_) => (500, "internal_error"),
            },
            Self::Task(error) => match error {
                TaskError::NotFound(_)
                | TaskError::TemplateNotFound(_)
                | TaskError::RepoNotFound(_) => (404, "not_found"),
                TaskError::Edit(_) | TaskError::Invalid(_) | TaskError::Attachment(_) => {
                    (400, "invalid_request")
                }
                TaskError::FileChanged(_) | TaskError::Conflict(_) => (409, "conflict"),
                TaskError::Failed(_) => (500, "internal_error"),
            },
        }
    }
}

impl From<GitError> for WorkspaceError {
    fn from(error: GitError) -> Self {
        Self::Git(error)
    }
}

impl From<OutsideWorkspaceError> for WorkspaceError {
    fn from(error: OutsideWorkspaceError) -> Self {
        Self::Outside(error)
    }
}

impl From<TaskError> for WorkspaceError {
    fn from(error: TaskError) -> Self {
        Self::Task(error)
    }
}

impl From<FileDiffError> for WorkspaceError {
    fn from(error: FileDiffError) -> Self {
        Self::Diff(error)
    }
}

impl From<FileImageError> for WorkspaceError {
    fn from(error: FileImageError) -> Self {
        Self::Image(error)
    }
}

impl From<CommitError> for WorkspaceError {
    fn from(error: CommitError) -> Self {
        match error {
            CommitError::Git(error) => Self::Git(error),
            CommitError::NothingToCommit => Self::NothingToCommit,
        }
    }
}

pub type WorkspaceResult<T> = Result<T, WorkspaceError>;

/// What `remove` answers: the task after the removal, and whether its branch went too.
#[derive(Clone, Debug)]
pub struct RemoveWorkspaceResult {
    pub task: Task,
    pub branch_deleted: bool,
}
