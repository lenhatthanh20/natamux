//! The routes of `packages/daemon/src/http/routes/terminals.ts` that the app calls.

use hyper::Method;
use natamux_proto::tasks::task::Ticket;
use natamux_proto::terminals::ended::{EndedShell, EndedShellList};
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, send};

/// The open shells of a task, with the lane and program of each.
pub async fn list_terminals(config: &Config, task_id: &str) -> Result<TerminalList, ClientError> {
    let path = format!("/tasks/{}/terminals", encode(task_id));
    call(config, Method::GET, &path).await
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
    send(config, Method::DELETE, &path, None).await.map(|_| ())
}

/// The last screens of the shells of a done or cancelled task.
pub async fn ended_shells(config: &Config, task_id: &str) -> Result<Vec<EndedShell>, ClientError> {
    let path = format!("/tasks/{}/ended-shells", encode(task_id));
    let list: EndedShellList = call(config, Method::GET, &path).await?;
    Ok(list.shells)
}
