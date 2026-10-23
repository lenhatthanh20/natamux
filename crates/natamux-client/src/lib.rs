//! Typed client of the Natamux daemon (L3): its routes, the live dashboard data and terminal
//! sockets. It runs on
//! its own small tokio runtime, so the app's main thread only reads channels and never waits on
//! the network.

pub mod config;
pub mod error;
mod git_methods;
mod http;
mod settings_methods;
pub mod agents {
    pub mod api;
}
pub mod events {
    pub mod live;
    pub mod stale;
}
pub mod projects {
    pub mod api;
}
pub mod stages {
    pub mod api;
}
pub mod tasks {
    pub mod api;
}
pub mod terminals {
    pub mod api;
    pub mod backoff;
    pub mod protocol;
    pub mod session;
}
pub mod workspaces {
    pub mod api;
}

use std::collections::BTreeMap;
use std::future::Future;
use std::sync::Arc;

use futures::channel::mpsc::UnboundedReceiver;
use natamux_proto::agents::activity::{TaskActivity, TaskUsage};
use natamux_proto::agents::start::StartAgentInput;
use natamux_proto::events::task_event::{StoredEvent, TaskEventInput};
use natamux_proto::projects::repo::{BranchList, LoadedSettings, RepoList, SkillList};
use natamux_proto::stages::actions::{StageActionInput, StageFile};
use natamux_proto::tasks::job::Job;
use natamux_proto::tasks::task::{
    DeleteTaskAnswer, NewTaskInput, Task, TaskChanges, TemplateTaskInput,
};
use natamux_proto::tasks::template::{TaskTemplate, TemplateInput};
use natamux_proto::terminals::ended::EndedShell;
use natamux_proto::terminals::list::TerminalList;
use natamux_proto::workspaces::workspace::{
    DiffStat, FanOutInput, NewWorkspaceInput, PreparingStep, RemoveWorkspaceAnswer,
};

pub use config::Config;
pub use error::ClientError;
pub use events::live::{DashboardFeed, FeedTimes, LinkStatus, TaskListEvent, TerminalRows};
pub use events::stale::DashboardEvent;
pub use http::request;
pub use hyper::Method;
pub use tasks::api::SendEventAnswer;
pub use terminals::protocol::{TerminalEvent, TerminalStatus};
pub use terminals::session::{SessionOptions, TerminalHandle};

/// A request that runs on the client's runtime; any executor can await it.
pub type Answer<T> = std::pin::Pin<Box<dyn Future<Output = Result<T, ClientError>> + Send>>;

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

    /// The dashboard data now and after every change, until the receiver is dropped.
    pub fn watch_dashboard(&self) -> DashboardFeed {
        self.watch_dashboard_every(FeedTimes::default())
    }

    /// `watch_dashboard` with other read times, for tests.
    pub fn watch_dashboard_every(&self, times: FeedTimes) -> DashboardFeed {
        events::live::watch_dashboard(self.runtime.handle(), self.config.clone(), times)
    }

    pub fn open_terminal(&self, options: SessionOptions) -> TerminalHandle {
        terminals::session::open(self.runtime.handle(), self.config.clone(), options)
    }

    pub fn close_terminal(&self, task_id: &str, terminal: &str) -> Answer<()> {
        let (task_id, terminal) = (task_id.to_string(), terminal.to_string());
        self.with_config(move |config| async move {
            terminals::api::close_terminal(&config, &task_id, &terminal).await
        })
    }

    pub fn list_terminals(&self, task_id: &str) -> Answer<TerminalList> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            terminals::api::list_terminals(&config, &task_id).await
        })
    }

    pub fn ended_shells(&self, task_id: &str) -> Answer<Vec<EndedShell>> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            terminals::api::ended_shells(&config, &task_id).await
        })
    }

    pub fn list_tasks(&self) -> Answer<Vec<Task>> {
        self.with_config(move |config| async move { tasks::api::list_tasks(&config).await })
    }

    pub fn create_task(&self, input: NewTaskInput) -> Answer<Task> {
        self.with_config(
            move |config| async move { tasks::api::create_task(&config, &input).await },
        )
    }

    pub fn list_templates(&self) -> Answer<Vec<TaskTemplate>> {
        self.with_config(move |config| async move { tasks::api::list_templates(&config).await })
    }

    pub fn create_from_template(
        &self,
        template_id: &str,
        input: TemplateTaskInput,
    ) -> Answer<Task> {
        let template_id = template_id.to_string();
        self.with_config(move |config| async move {
            tasks::api::create_from_template(&config, &template_id, &input).await
        })
    }

    pub fn list_jobs(&self) -> Answer<Vec<Job>> {
        self.with_config(move |config| async move { tasks::api::list_jobs(&config).await })
    }

    pub fn list_events(&self, task_id: &str) -> Answer<Vec<StoredEvent>> {
        let task_id = task_id.to_string();
        self.with_config(
            move |config| async move { tasks::api::list_events(&config, &task_id).await },
        )
    }

    pub fn send_event(&self, task_id: &str, input: TaskEventInput) -> Answer<SendEventAnswer> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            tasks::api::send_event(&config, &task_id, &input).await
        })
    }

    pub fn rename_task(&self, task_id: &str, title: &str) -> Answer<Task> {
        let (task_id, title) = (task_id.to_string(), title.to_string());
        self.with_config(move |config| async move {
            tasks::api::rename_task(&config, &task_id, &title).await
        })
    }

    pub fn update_task(&self, task_id: &str, changes: TaskChanges) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            tasks::api::update_task(&config, &task_id, &changes).await
        })
    }

    pub fn save_template(&self, input: TemplateInput) -> Answer<TaskTemplate> {
        self.with_config(
            move |config| async move { tasks::api::save_template(&config, &input).await },
        )
    }

    pub fn create_issue(&self, task_id: &str) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(
            move |config| async move { tasks::api::create_issue(&config, &task_id).await },
        )
    }

    pub fn stage_action(&self, task_id: &str, input: StageActionInput) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            stages::api::stage_action(&config, &task_id, &input).await
        })
    }

    pub fn set_workflow(&self, task_id: &str, stage: &str, on: bool) -> Answer<Task> {
        let (task_id, stage) = (task_id.to_string(), stage.to_string());
        self.with_config(move |config| async move {
            stages::api::set_workflow(&config, &task_id, &stage, on).await
        })
    }

    pub fn set_ci(&self, task_id: &str, stage: &str, on: bool) -> Answer<Task> {
        let (task_id, stage) = (task_id.to_string(), stage.to_string());
        self.with_config(move |config| async move {
            stages::api::set_ci(&config, &task_id, &stage, on).await
        })
    }

    pub fn set_skip(&self, task_id: &str, stage: &str, on: bool) -> Answer<Task> {
        let (task_id, stage) = (task_id.to_string(), stage.to_string());
        self.with_config(move |config| async move {
            stages::api::set_skip(&config, &task_id, &stage, on).await
        })
    }

    pub fn list_stage_files(&self, task_id: &str) -> Answer<Vec<StageFile>> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            stages::api::list_stage_files(&config, &task_id).await
        })
    }

    pub fn fan_out(&self, task_id: &str, input: FanOutInput) -> Answer<Vec<Task>> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::fan_out(&config, &task_id, &input).await
        })
    }

    pub fn refresh_pull_request(&self, task_id: &str) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::refresh_pull_request(&config, &task_id).await
        })
    }

    pub fn delete_task(&self, task_id: &str) -> Answer<DeleteTaskAnswer> {
        let task_id = task_id.to_string();
        self.with_config(
            move |config| async move { tasks::api::delete_task(&config, &task_id).await },
        )
    }

    pub fn list_repos(&self) -> Answer<RepoList> {
        self.with_config(move |config| async move { projects::api::list_repos(&config).await })
    }

    pub fn remove_repo(&self, repo_id: &str) -> Answer<()> {
        let repo_id = repo_id.to_string();
        self.with_config(move |config| async move {
            projects::api::remove_repo(&config, &repo_id).await
        })
    }

    pub fn list_branches(&self, repo: Option<&str>) -> Answer<BranchList> {
        let repo = repo.map(str::to_string);
        self.with_config(move |config| async move {
            projects::api::list_branches(&config, repo.as_deref()).await
        })
    }

    pub fn load_settings(&self, repo_id: &str) -> Answer<LoadedSettings> {
        let repo_id = repo_id.to_string();
        self.with_config(move |config| async move {
            projects::api::load_settings(&config, &repo_id).await
        })
    }

    pub fn list_skills(&self, repo_id: &str) -> Answer<SkillList> {
        let repo_id = repo_id.to_string();
        self.with_config(move |config| async move {
            projects::api::list_skills(&config, &repo_id).await
        })
    }

    pub fn new_workspace(&self, input: NewWorkspaceInput) -> Answer<Task> {
        self.with_config(move |config| async move {
            workspaces::api::new_workspace(&config, &input).await
        })
    }

    pub fn create_workspace(&self, task_id: &str, base: Option<&str>) -> Answer<Task> {
        let (task_id, base) = (task_id.to_string(), base.map(str::to_string));
        self.with_config(move |config| async move {
            workspaces::api::create_workspace(&config, &task_id, base.as_deref()).await
        })
    }

    pub fn push(&self, task_id: &str) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(
            move |config| async move { workspaces::api::push(&config, &task_id).await },
        )
    }

    pub fn open_pull_request(&self, task_id: &str) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::open_pull_request(&config, &task_id).await
        })
    }

    pub fn remove_workspace(
        &self,
        task_id: &str,
        delete_branch: bool,
    ) -> Answer<RemoveWorkspaceAnswer> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::remove_workspace(&config, &task_id, delete_branch).await
        })
    }

    pub fn list_preparing(&self) -> Answer<BTreeMap<String, PreparingStep>> {
        self.with_config(
            move |config| async move { workspaces::api::list_preparing(&config).await },
        )
    }

    pub fn diff_stats(&self) -> Answer<BTreeMap<String, DiffStat>> {
        self.with_config(move |config| async move { workspaces::api::diff_stats(&config).await })
    }

    pub fn activity(&self) -> Answer<BTreeMap<String, TaskActivity>> {
        self.with_config(move |config| async move { agents::api::activity(&config).await })
    }

    pub fn usage(&self) -> Answer<BTreeMap<String, TaskUsage>> {
        self.with_config(move |config| async move { agents::api::usage(&config).await })
    }

    pub fn start_agent(&self, task_id: &str, input: StartAgentInput) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            agents::api::start_agent(&config, &task_id, &input).await
        })
    }

    pub fn reopen_agent(&self, task_id: &str) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(
            move |config| async move { agents::api::reopen_agent(&config, &task_id).await },
        )
    }

    /// Runs a request with the config on the client's runtime.
    fn with_config<T, F>(&self, request: impl FnOnce(Arc<Config>) -> F) -> Answer<T>
    where
        T: Send + 'static,
        F: Future<Output = Result<T, ClientError>> + Send + 'static,
    {
        Box::pin(self.on_runtime(request(self.config.clone())))
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
