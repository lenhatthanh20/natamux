//! The HTTP routes the app calls (`packages/daemon/src/http/routes`).

use hyper::Method;
use natamux_proto::tasks::task::{Task, TaskList, Ticket};
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, send};

pub async fn list_tasks(config: &Config) -> Result<Vec<Task>, ClientError> {
    let list: TaskList = call(config, Method::GET, "/tasks").await?;
    Ok(list.tasks)
}

/// A one-time ticket for the terminal socket of a task. It is good for 30 s.
pub async fn terminal_ticket(config: &Config, task_id: &str) -> Result<String, ClientError> {
    let path = format!("/tasks/{}/terminal/ticket", encode(task_id));
    let ticket: Ticket = call(config, Method::POST, &path).await?;
    Ok(ticket.ticket)
}

/// Ends one terminal of a task and its shell.
pub async fn close_terminal(
    config: &Config,
    task_id: &str,
    terminal: &str,
) -> Result<(), ClientError> {
    let path = format!("/tasks/{}/terminals/{}", encode(task_id), encode(terminal));
    send(config, Method::DELETE, &path).await.map(|_| ())
}
