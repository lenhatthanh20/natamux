//! What the Docs tab writes: Cancel, Save of a stage file, and the download of an attachment.

use std::path::PathBuf;

use natamux_proto::tasks::attachments::Attachment;
use ui::prelude::*;

use super::docs_rules::{cancel_question, is_stale, save_question};
use super::docs_tab::DocsTab;
use crate::app::source::reason;
use crate::components::confirm::Confirm;
use crate::components::toast_rules::ToastTone;

impl DocsTab {
    fn drop_draft(&mut self, name: &str, cx: &mut Context<Self>) {
        let task = self.task_id.clone();
        self.store
            .update(cx, |store, cx| store.set_doc(&task, name, None, cx));
    }

    /// Cancel: a changed text asks first, an unchanged one closes at once.
    pub(super) fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.current() else {
            return;
        };
        let dirty = self
            .store
            .read(cx)
            .doc(&self.task_id, &name)
            .is_some_and(|draft| draft.is_dirty());
        let Some(question) = cancel_question(&name, dirty) else {
            self.drop_draft(&name, cx);
            return;
        };
        let this = cx.weak_entity();
        Confirm::new(
            "Drop your changes?",
            question,
            "Drop changes",
            move |_, cx| {
                this.update(cx, |this, cx| this.drop_draft(&name, cx)).ok();
            },
        )
        .open(window, cx);
    }

    /// Save: a file that changed under the edit asks first.
    pub(super) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(name) = self.current().filter(|_| !self.saving) else {
            return;
        };
        let Some(draft) = self.store.read(cx).doc(&self.task_id, &name).cloned() else {
            return;
        };
        let stale = is_stale(&draft, self.modified_of(&name).as_deref());
        let Some(question) = save_question(&name, stale) else {
            self.send_save(name, draft.text, cx);
            return;
        };
        let this = cx.weak_entity();
        Confirm::new("Replace the file?", question, "Replace", move |_, cx| {
            let (name, text) = (name.clone(), draft.text.clone());
            this.update(cx, |this, cx| this.send_save(name, text, cx))
                .ok();
        })
        .open(window, cx);
    }

    fn send_save(&mut self, name: String, text: String, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        self.saving = true;
        cx.notify();
        let task = self.task_id.clone();
        let saved = source.write_stage_file(&task, &name, text);
        cx.spawn(async move |this, cx| {
            let saved = saved.await.map_err(|error| reason(&error));
            if let Err(message) = saved {
                this.update(cx, |this, cx| {
                    this.saving = false;
                    this.toast(
                        &format!("Could not save {name}."),
                        ToastTone::Error,
                        Some(message),
                        cx,
                    );
                    cx.notify();
                })
                .ok();
                return;
            }
            // Read the new list and text before the draft goes, so the old text does not flash.
            let list = source.list_stage_files(&task).await.map_err(|e| reason(&e));
            let modified = list.as_ref().ok().and_then(|list| {
                list.iter()
                    .find(|file| file.name == name)
                    .map(|file| file.modified_at.clone())
            });
            let text = match modified {
                Some(_) => Some(source.read_stage_file(&task, &name).await),
                None => None,
            };
            this.update(cx, |this, cx| {
                this.toast(&format!("Saved {name}."), ToastTone::Success, None, cx);
                if let (Some(modified), Some(Ok(file))) = (modified, text) {
                    this.apply_text((name.clone(), modified), Ok(file.text), cx);
                }
                this.apply_list(list, cx);
                this.saving = false;
                this.drop_draft(&name, cx);
            })
            .ok();
        })
        .detach();
    }

    fn toast(&self, text: &str, tone: ToastTone, detail: Option<String>, cx: &mut Context<Self>) {
        self.dashboard
            .update(cx, |dashboard, cx| dashboard.toast(text, tone, detail, cx));
    }

    /// Asks where to save, then writes the bytes of the attachment there.
    pub(super) fn download(&mut self, item: Attachment, cx: &mut Context<Self>) {
        let Some(source) = self.dashboard.read(cx).source().cloned() else {
            return;
        };
        let home = std::env::var_os("HOME").map_or_else(PathBuf::new, PathBuf::from);
        let path = cx.prompt_for_new_path(&home, Some(&item.name));
        let bytes = source.read_attachment(&self.task_id, &item.name);
        let name = item.name;
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = path.await else {
                return;
            };
            let written = match bytes.await.map_err(|error| reason(&error)) {
                Ok(bytes) => cx
                    .background_executor()
                    .spawn(async move { std::fs::write(&path, bytes) })
                    .await
                    .map_err(|error| error.to_string()),
                Err(message) => Err(message),
            };
            this.update(cx, |this, cx| match written {
                Ok(()) => this.toast(&format!("Downloaded {name}."), ToastTone::Success, None, cx),
                Err(message) => this.toast(
                    &format!("Could not download {name}."),
                    ToastTone::Error,
                    Some(message),
                    cx,
                ),
            })
            .ok();
        })
        .detach();
    }
}
