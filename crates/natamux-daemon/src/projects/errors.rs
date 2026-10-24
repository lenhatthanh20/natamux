//! The errors of the project list and settings, with the Node daemon's texts (`repo-service.ts`,
//! `project-settings.ts`) and its HTTP mapping (`http/errors.ts`).

use natamux_core::zod::ZodError;
use natamux_git::exec::GitError;

use crate::tasks::errors::TaskError;

#[derive(Debug)]
pub enum RepoError {
    /// 400 with zod's issues.
    Invalid(ZodError),
    /// `RepoInputError`: 400 `invalid_request`.
    Input(String),
    /// `GitError`: 409 `conflict`.
    Git(GitError),
    /// `RepoInUseError`: 409 `conflict`.
    InUse(String),
    /// `RepoNotFoundError`: 404 `not_found`.
    NotFound(String),
    /// `SettingsInputError`: 400 `invalid_request`.
    SettingsInput(String),
    /// `SettingsChangedError`: 409 `conflict`.
    SettingsChanged(String),
    /// 500: the disk failed.
    Failed(String),
}

impl RepoError {
    /// Node's `ERROR_ANSWERS`. `Invalid` also carries zod's issues in its answer.
    pub fn status_and_code(&self) -> (u16, &'static str) {
        match self {
            Self::Invalid(_) | Self::Input(_) | Self::SettingsInput(_) => (400, "invalid_request"),
            Self::NotFound(_) => (404, "not_found"),
            Self::Git(_) | Self::InUse(_) | Self::SettingsChanged(_) => (409, "conflict"),
            Self::Failed(_) => (500, "internal_error"),
        }
    }
}

impl std::fmt::Display for RepoError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Invalid(error) => std::fmt::Display::fmt(error, f),
            Self::Git(error) => std::fmt::Display::fmt(error, f),
            Self::Input(text)
            | Self::InUse(text)
            | Self::NotFound(text)
            | Self::SettingsInput(text)
            | Self::SettingsChanged(text)
            | Self::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for RepoError {}

impl From<ZodError> for RepoError {
    fn from(error: ZodError) -> RepoError {
        RepoError::Invalid(error)
    }
}

impl From<GitError> for RepoError {
    fn from(error: GitError) -> RepoError {
        RepoError::Git(error)
    }
}

impl From<std::io::Error> for RepoError {
    fn from(error: std::io::Error) -> RepoError {
        RepoError::Failed(error.to_string())
    }
}

impl From<TaskError> for RepoError {
    fn from(error: TaskError) -> RepoError {
        match error {
            TaskError::RepoNotFound(text) => RepoError::NotFound(text),
            TaskError::Invalid(error) => RepoError::Invalid(error),
            other => RepoError::Failed(other.to_string()),
        }
    }
}
