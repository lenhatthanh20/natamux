//! Saving the open file (`FileEditor.tsx` `save`): `PUT /tasks/:id/file` with the draft and the
//! version it began on.

use natamux_client::ClientError;
use natamux_proto::workspaces::files::SavedFile;
use ui::prelude::*;

use super::file_drafts::{FileDraft, after_save, can_save};
use super::file_editor::FileEditor;
use crate::app::source::reason;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{push_toast, toast};

impl FileEditor {
    /// Mod+S or the Save button. Ignored without a draft, while the banner shows or while a save
    /// runs.
    pub(super) fn save(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft(cx);
        let stale = self.stale_now(cx);
        let Some(draft) = draft.filter(|d| can_save(Some(d), stale, self.saving)) else {
            return;
        };
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let answer = source.write_workspace_file(
            &self.task_id,
            &self.path,
            draft.text.clone(),
            draft.version.clone(),
        );
        self.saving = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            let answer = answer.await;
            this.update(cx, |this, cx| this.saved(draft, answer, cx))
                .ok();
        })
        .detach();
    }

    fn saved(
        &mut self,
        sent: FileDraft,
        answer: Result<SavedFile, ClientError>,
        cx: &mut Context<Self>,
    ) {
        self.saving = false;
        let name = self
            .path
            .rsplit('/')
            .next()
            .unwrap_or(&self.path)
            .to_owned();
        match answer {
            Ok(saved) => {
                if let Some(disk) = self.disk.as_mut() {
                    disk.text = Some(sent.text.clone());
                    disk.version = saved.version.clone();
                }
                let now = self.draft(cx);
                let next = after_save(now.as_ref(), &sent.text, &saved.version);
                let (task, path) = (self.task_id.clone(), self.path.clone());
                self.store
                    .update(cx, |store, cx| store.set_file(&task, &path, next, cx));
                toast(&format!("Saved {name}."), ToastTone::Success, cx);
            }
            // The new version shows the banner that asks what to keep.
            Err(error) if error.is_conflict() => self.refresh(cx),
            Err(error) => push_toast(
                &format!("Could not save {name}."),
                ToastTone::Error,
                Some(reason(&error)),
                Vec::new(),
                cx,
            ),
        }
        cx.notify();
    }
}
