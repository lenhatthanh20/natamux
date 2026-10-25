//! The errors of the stage engine: `StageActionError`, `StageInputError` (`stage-actions.ts`) and
//! `StageFileError` (`stage-files.ts`), with the errors of the services it calls.

use std::fmt;

use crate::agents::errors::AgentError;
use crate::tasks::errors::TaskError;
use crate::terminals::types::TerminalError;

#[derive(Debug)]
pub enum StageError {
    /// 409 `conflict`: the action does not fit the task now.
    Action(String),
    /// 400 `invalid_request`: the action names something the task does not have.
    Input(String),
    /// 400 `invalid_request`: a stage file name or size that is not allowed.
    File(String),
    Task(TaskError),
    Terminal(TerminalError),
    Agent(AgentError),
}

pub type StageResult<T> = Result<T, StageError>;

impl fmt::Display for StageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StageError::Action(message)
            | StageError::Input(message)
            | StageError::File(message) => {
                write!(f, "{message}")
            }
            StageError::Task(error) => write!(f, "{error}"),
            StageError::Terminal(error) => write!(f, "{error}"),
            StageError::Agent(error) => write!(f, "{error}"),
        }
    }
}

impl From<TaskError> for StageError {
    fn from(error: TaskError) -> Self {
        StageError::Task(error)
    }
}

impl From<TerminalError> for StageError {
    fn from(error: TerminalError) -> Self {
        StageError::Terminal(error)
    }
}

impl From<AgentError> for StageError {
    fn from(error: AgentError) -> Self {
        match error {
            AgentError::Task(error) => StageError::Task(error),
            AgentError::Terminal(error) => StageError::Terminal(error),
            other => StageError::Agent(other),
        }
    }
}

impl From<std::io::Error> for StageError {
    fn from(error: std::io::Error) -> Self {
        StageError::Task(TaskError::Failed(error.to_string()))
    }
}

pub fn action(message: impl Into<String>) -> StageError {
    StageError::Action(message.into())
}
