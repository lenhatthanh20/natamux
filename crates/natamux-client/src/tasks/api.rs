//! The routes of `packages/daemon/src/http/routes/tasks.ts` that the app calls.

use hyper::Method;
use natamux_proto::events::task_event::{EventList, StoredEvent, TaskEventInput};
use natamux_proto::tasks::attachments::{Attachment, AttachmentList};
use natamux_proto::tasks::job::{Job, JobList};
use natamux_proto::tasks::task::{
    DeleteTaskAnswer, NewTaskInput, Task, TaskAnswer, TaskChanges, TaskList, TemplateTaskInput,
};
use natamux_proto::tasks::template::{TaskTemplate, TemplateAnswer, TemplateInput, TemplateList};
use natamux_proto::terminals::query::encode;
use serde::Deserialize;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with, send};

/// The answer of `POST /tasks/:id/events`. `accepted`: the event changed the task.
#[derive(Clone, Debug, Deserialize)]
pub struct SendEventAnswer {
    pub task: Task,
    #[serde(default)]
    pub accepted: bool,
}

pub async fn list_tasks(config: &Config) -> Result<Vec<Task>, ClientError> {
    let list: TaskList = call(config, Method::GET, "/tasks").await?;
    Ok(list.tasks)
}

pub async fn create_task(config: &Config, input: &NewTaskInput) -> Result<Task, ClientError> {
    let answer: TaskAnswer = call_with(config, Method::POST, "/tasks", input).await?;
    Ok(answer.task)
}

pub async fn list_templates(config: &Config) -> Result<Vec<TaskTemplate>, ClientError> {
    let list: TemplateList = call(config, Method::GET, "/templates").await?;
    Ok(list.templates)
}

pub async fn create_from_template(
    config: &Config,
    template_id: &str,
    input: &TemplateTaskInput,
) -> Result<Task, ClientError> {
    let path = format!("/templates/{}/tasks", encode(template_id));
    let answer: TaskAnswer = call_with(config, Method::POST, &path, input).await?;
    Ok(answer.task)
}

pub async fn list_jobs(config: &Config) -> Result<Vec<Job>, ClientError> {
    let list: JobList = call(config, Method::GET, "/jobs").await?;
    Ok(list.jobs)
}

pub async fn list_events(config: &Config, task_id: &str) -> Result<Vec<StoredEvent>, ClientError> {
    let path = format!("/tasks/{}/events", encode(task_id));
    let list: EventList = call(config, Method::GET, &path).await?;
    Ok(list.events)
}

pub async fn send_event(
    config: &Config,
    task_id: &str,
    input: &TaskEventInput,
) -> Result<SendEventAnswer, ClientError> {
    let path = format!("/tasks/{}/events", encode(task_id));
    call_with(config, Method::POST, &path, input).await
}

pub async fn rename_task(config: &Config, task_id: &str, title: &str) -> Result<Task, ClientError> {
    let changes = TaskChanges {
        title: Some(title.to_string()),
        ..TaskChanges::default()
    };
    update_task(config, task_id, &changes).await
}

/// Changes the fields that are set; the others stay.
pub async fn update_task(
    config: &Config,
    task_id: &str,
    changes: &TaskChanges,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}", encode(task_id));
    let answer: TaskAnswer = call_with(config, Method::PATCH, &path, changes).await?;
    Ok(answer.task)
}

pub async fn save_template(
    config: &Config,
    input: &TemplateInput,
) -> Result<TaskTemplate, ClientError> {
    let answer: TemplateAnswer = call_with(config, Method::POST, "/templates", input).await?;
    Ok(answer.template)
}

/// Makes the issue of the task in the project's tracker.
pub async fn create_issue(config: &Config, task_id: &str) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/issue", encode(task_id));
    let answer: TaskAnswer = call(config, Method::POST, &path).await?;
    Ok(answer.task)
}

/// Deletes the task, and its workspace with its branch. Uncommitted changes refuse it.
pub async fn delete_task(config: &Config, task_id: &str) -> Result<DeleteTaskAnswer, ClientError> {
    let path = format!("/tasks/{}", encode(task_id));
    call(config, Method::DELETE, &path).await
}

/// The files kept with the task.
pub async fn list_attachments(
    config: &Config,
    task_id: &str,
) -> Result<Vec<Attachment>, ClientError> {
    let path = format!("/tasks/{}/attachments", encode(task_id));
    let list: AttachmentList = call(config, Method::GET, &path).await?;
    Ok(list.attachments)
}

/// The raw bytes of one attachment. 404: "<id> has no <name>".
pub async fn read_attachment(
    config: &Config,
    task_id: &str,
    name: &str,
) -> Result<Vec<u8>, ClientError> {
    let path = format!("/tasks/{}/attachments/{}", encode(task_id), encode(name));
    Ok(send(config, Method::GET, &path, None).await?.to_vec())
}
