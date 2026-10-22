//! Typed client of the Natamux daemon (L3): the live task list and terminal sockets. It runs on
//! its own small tokio runtime, so the app's main thread only reads channels and never waits on
//! the network.

pub mod config;
pub mod error;
mod http;
pub mod events {
    pub mod live;
}
pub mod tasks {
    pub mod api;
}
pub mod terminals {
    pub mod backoff;
    pub mod protocol;
    pub mod session;
}

use std::future::Future;
use std::sync::Arc;

use futures::channel::mpsc::UnboundedReceiver;
use natamux_proto::tasks::task::Task;

pub use config::Config;
pub use error::ClientError;
pub use events::live::{LinkStatus, TaskListEvent};
pub use terminals::protocol::{TerminalEvent, TerminalStatus};
pub use terminals::session::{SessionOptions, TerminalHandle};

pub struct Client {
    runtime: tokio::runtime::Runtime,
    config: Arc<Config>,
}

impl Client {
    pub fn new(config: Config) -> Result<Client, ClientError> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("natamux-client")
            .enable_all()
            .build()
            .map_err(|error| ClientError::Unreachable {
                url: config.base_url.clone(),
                reason: format!("cannot start the network threads: {error}"),
            })?;
        Ok(Client {
            runtime,
            config: Arc::new(config),
        })
    }

    pub fn config(&self) -> &Config {
        &self.config
    }

    /// The task list now and after every change, until the receiver is dropped.
    pub fn watch_tasks(&self) -> UnboundedReceiver<TaskListEvent> {
        events::live::watch(self.runtime.handle(), self.config.clone())
    }

    pub fn list_tasks(&self) -> impl Future<Output = Result<Vec<Task>, ClientError>> + use<> {
        let config = self.config.clone();
        self.on_runtime(async move { tasks::api::list_tasks(&config).await })
    }

    pub fn open_terminal(&self, options: SessionOptions) -> TerminalHandle {
        terminals::session::open(self.runtime.handle(), self.config.clone(), options)
    }

    pub fn close_terminal(
        &self,
        task_id: &str,
        terminal: &str,
    ) -> impl Future<Output = Result<(), ClientError>> + use<> {
        let config = self.config.clone();
        let (task_id, terminal) = (task_id.to_string(), terminal.to_string());
        self.on_runtime(
            async move { tasks::api::close_terminal(&config, &task_id, &terminal).await },
        )
    }

    /// Runs a future on the client's runtime and gives back one that any executor can await.
    fn on_runtime<T, F>(
        &self,
        future: F,
    ) -> impl Future<Output = Result<T, ClientError>> + use<T, F>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, ClientError>> + Send + 'static,
    {
        let task = self.runtime.spawn(future);
        let base_url = self.config.base_url.clone();
        async move {
            task.await.unwrap_or_else(|error| {
                Err(ClientError::Unreachable {
                    url: base_url,
                    reason: format!("the request stopped: {error}"),
                })
            })
        }
    }
}
