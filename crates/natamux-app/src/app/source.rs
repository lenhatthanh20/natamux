//! Where the app's tasks and terminals come from: the daemon, or the fixed data of
//! `--fake-daemon` for CI screenshots.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use futures::channel::mpsc::UnboundedReceiver;
use natamux_client::{Client, ClientError, SessionOptions, TaskListEvent, TerminalHandle};

use super::fake_daemon;

pub enum Source {
    Daemon(Arc<Client>),
    FakeDaemon,
}

impl Source {
    pub fn watch_tasks(&self) -> UnboundedReceiver<TaskListEvent> {
        match self {
            Source::Daemon(client) => client.watch_tasks(),
            Source::FakeDaemon => fake_daemon::tasks(),
        }
    }

    pub fn open_terminal(&self, options: SessionOptions) -> TerminalHandle {
        match self {
            Source::Daemon(client) => client.open_terminal(options),
            Source::FakeDaemon => TerminalHandle::detached(fake_daemon::terminal()),
        }
    }

    /// Ends a terminal of a task and its shell.
    pub fn close_terminal(
        &self,
        task_id: &str,
        terminal: &str,
    ) -> Pin<Box<dyn Future<Output = Result<(), ClientError>>>> {
        match self {
            Source::Daemon(client) => Box::pin(client.close_terminal(task_id, terminal)),
            Source::FakeDaemon => Box::pin(async { Ok(()) }),
        }
    }
}
