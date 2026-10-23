//! The `Client` calls of the Files tab, the editor tab and the Docs tab.

use natamux_proto::stages::actions::StageFileText;
use natamux_proto::tasks::attachments::Attachment;
use natamux_proto::workspaces::files::{SavedFile, TreeEntry, WorkspaceFile};

use crate::{Answer, Client, stages, tasks, workspaces};

impl Client {
    /// One folder of the worktree; the root is the empty path.
    pub fn list_folder(&self, task_id: &str, folder: &str) -> Answer<Vec<TreeEntry>> {
        let (task_id, folder) = (task_id.to_string(), folder.to_string());
        self.with_config(move |config| async move {
            workspaces::api::list_folder(&config, &task_id, &folder).await
        })
    }

    pub fn read_workspace_file(&self, task_id: &str, path: &str) -> Answer<WorkspaceFile> {
        let (task_id, path) = (task_id.to_string(), path.to_string());
        self.with_config(move |config| async move {
            workspaces::api::read_workspace_file(&config, &task_id, &path).await
        })
    }

    /// `version`: from `read_workspace_file`; the daemon answers 409 when the file changed since.
    pub fn write_workspace_file(
        &self,
        task_id: &str,
        path: &str,
        text: String,
        version: String,
    ) -> Answer<SavedFile> {
        let (task_id, path) = (task_id.to_string(), path.to_string());
        self.with_config(move |config| async move {
            workspaces::api::write_workspace_file(&config, &task_id, &path, &text, &version).await
        })
    }

    pub fn read_stage_file(&self, task_id: &str, name: &str) -> Answer<StageFileText> {
        let (task_id, name) = (task_id.to_string(), name.to_string());
        self.with_config(move |config| async move {
            stages::api::read_stage_file(&config, &task_id, &name).await
        })
    }

    pub fn write_stage_file(
        &self,
        task_id: &str,
        name: &str,
        text: String,
    ) -> Answer<StageFileText> {
        let (task_id, name) = (task_id.to_string(), name.to_string());
        self.with_config(move |config| async move {
            stages::api::write_stage_file(&config, &task_id, &name, &text).await
        })
    }

    pub fn list_attachments(&self, task_id: &str) -> Answer<Vec<Attachment>> {
        let task_id = task_id.to_string();
        self.with_config(move |config| async move {
            tasks::api::list_attachments(&config, &task_id).await
        })
    }

    pub fn read_attachment(&self, task_id: &str, name: &str) -> Answer<Vec<u8>> {
        let (task_id, name) = (task_id.to_string(), name.to_string());
        self.with_config(move |config| async move {
            tasks::api::read_attachment(&config, &task_id, &name).await
        })
    }
}
