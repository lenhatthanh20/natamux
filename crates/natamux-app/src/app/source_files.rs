//! The file calls of screen 6 (Files tab, editor tab, Docs tab), from the daemon or the fake.

use natamux_client::Answer;
use natamux_proto::stages::actions::StageFileText;
use natamux_proto::tasks::attachments::Attachment;
use natamux_proto::workspaces::files::{SavedFile, TreeEntry, WorkspaceFile};

use super::Source;

impl Source {
    /// One folder of the worktree; the root is the empty path.
    pub fn list_folder(&self, task_id: &str, folder: &str) -> Answer<Vec<TreeEntry>> {
        self.ask(
            |c| c.list_folder(task_id, folder),
            |f| f.list_folder(task_id, folder),
        )
    }

    pub fn read_workspace_file(&self, task_id: &str, path: &str) -> Answer<WorkspaceFile> {
        self.ask(
            |c| c.read_workspace_file(task_id, path),
            |f| f.read_workspace_file(task_id, path),
        )
    }

    /// `version` is what the read answered; the daemon refuses an old one with 409.
    pub fn write_workspace_file(
        &self,
        task_id: &str,
        path: &str,
        text: String,
        version: String,
    ) -> Answer<SavedFile> {
        let (fake_text, fake_version) = (text.clone(), version.clone());
        self.ask(
            |c| c.write_workspace_file(task_id, path, text, version),
            |f| f.write_workspace_file(task_id, path, fake_text, fake_version),
        )
    }

    pub fn read_stage_file(&self, task_id: &str, name: &str) -> Answer<StageFileText> {
        self.ask(
            |c| c.read_stage_file(task_id, name),
            |f| f.read_stage_file(task_id, name),
        )
    }

    pub fn write_stage_file(
        &self,
        task_id: &str,
        name: &str,
        text: String,
    ) -> Answer<StageFileText> {
        let fake = text.clone();
        self.ask(
            |c| c.write_stage_file(task_id, name, text),
            |f| f.write_stage_file(task_id, name, fake),
        )
    }

    pub fn list_attachments(&self, task_id: &str) -> Answer<Vec<Attachment>> {
        self.ask(
            |c| c.list_attachments(task_id),
            |f| f.list_attachments(task_id),
        )
    }

    pub fn read_attachment(&self, task_id: &str, name: &str) -> Answer<Vec<u8>> {
        self.ask(
            |c| c.read_attachment(task_id, name),
            |f| f.read_attachment(task_id, name),
        )
    }
}
