//! The errors of the task services, with the Node daemon's texts (`task-inputs.ts`,
//! `attachments.ts`, `template-service.ts`).

use natamux_core::zod::ZodError;
use natamux_store::task_sync::StoreError;

#[derive(Debug)]
pub enum TaskError {
    /// 404 `Task <id> does not exist`.
    NotFound(String),
    /// 400: a change that does not fit, such as a used id.
    Edit(String),
    /// 409: the task file changed by hand since it was read.
    FileChanged(String),
    /// 400 with zod's issues.
    Invalid(ZodError),
    /// 400: an attachment over a limit or with a bad name.
    Attachment(String),
    /// 404 `Template <id> does not exist`.
    TemplateNotFound(String),
    /// 404 `Repository <id> is not known. Add it first`, or no default project.
    RepoNotFound(String),
    /// 409: an event that would enter a branch, which needs the stage engine.
    Conflict(String),
    /// 500: the disk or the index failed.
    Failed(String),
}

impl std::fmt::Display for TaskError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TaskError::NotFound(id) => write!(f, "Task {id} does not exist"),
            TaskError::FileChanged(id) => write!(
                f,
                "The file of task {id} was changed outside Natamux. It is read again; try again"
            ),
            TaskError::TemplateNotFound(id) => write!(f, "Template {id} does not exist"),
            TaskError::Invalid(error) => write!(f, "{}", error.describe()),
            TaskError::Edit(text)
            | TaskError::Attachment(text)
            | TaskError::RepoNotFound(text)
            | TaskError::Conflict(text)
            | TaskError::Failed(text) => f.write_str(text),
        }
    }
}

impl std::error::Error for TaskError {}

impl From<ZodError> for TaskError {
    fn from(error: ZodError) -> TaskError {
        TaskError::Invalid(error)
    }
}

impl From<StoreError> for TaskError {
    fn from(error: StoreError) -> TaskError {
        match error {
            StoreError::FileChanged(id) => TaskError::FileChanged(id),
            StoreError::Failed(text) => TaskError::Failed(text),
        }
    }
}

impl From<std::io::Error> for TaskError {
    fn from(error: std::io::Error) -> TaskError {
        TaskError::Failed(error.to_string())
    }
}

pub type TaskResult<T> = Result<T, TaskError>;
