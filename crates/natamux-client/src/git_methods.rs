//! The `Client` calls of the Changes and History tabs.

use natamux_proto::agents::start::AgentMessage;
use natamux_proto::tasks::task::Task;
use natamux_proto::workspaces::changes::{
    CommitAnswer, CommitInput, CommitMessageDraft, FileDiff, ImageSide, RollbackInput,
    RollbackResult,
};
use natamux_proto::workspaces::workspace::{ChangeEntry, GitGraph};

use crate::{Answer, Client, agents, workspaces};

impl Client {
    /// `fresh`: read git again, not the daemon's short cache (the Refresh button).
    pub fn list_changes(&self, task_id: &str, fresh: bool) -> Answer<Vec<ChangeEntry>> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::list_changes(&config, &task_id, fresh).await
        })
    }

    pub fn graph(&self, task_id: &str, fresh: bool) -> Answer<GitGraph> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::graph(&config, &task_id, fresh).await
        })
    }

    pub fn file_diff(&self, task_id: &str, path: &str) -> Answer<FileDiff> {
        let (task_id, path) = (task_id.to_string(), path.to_string());
        self.with_config(move |config| async move {
            workspaces::api::file_diff(&config, &task_id, &path).await
        })
    }

    pub fn file_image(&self, task_id: &str, path: &str, side: ImageSide) -> Answer<Vec<u8>> {
        let (task_id, path) = (task_id.to_string(), path.to_string());
        self.with_config(move |config| async move {
            workspaces::api::file_image(&config, &task_id, &path, side).await
        })
    }

    pub fn commit(&self, task_id: &str, message: String) -> Answer<CommitAnswer> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            let input = CommitInput { message };
            workspaces::api::commit(&config, &task_id, &input).await
        })
    }

    pub fn commit_message(&self, task_id: &str) -> Answer<CommitMessageDraft> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::commit_message(&config, &task_id).await
        })
    }

    pub fn rollback(&self, task_id: &str, paths: Vec<String>) -> Answer<RollbackResult> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            let input = RollbackInput { paths };
            workspaces::api::rollback(&config, &task_id, &input).await
        })
    }

    pub fn undo_rollback(&self, task_id: &str) -> Answer<RollbackResult> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            workspaces::api::undo_rollback(&config, &task_id).await
        })
    }

    pub fn send_to_agent(&self, task_id: &str, text: String) -> Answer<Task> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            agents::api::send_to_agent(&config, &task_id, &AgentMessage { text }).await
        })
    }
}
