//! The unsaved edits of the app, while it runs: the files of the editor tab and the stage files of
//! the Docs tab (`useFileDrafts` and the drafts `DocsTab.tsx` keeps). One entity, so the Files
//! tree, the Docs list and the quit question see the same drafts; views observe it.

use std::collections::BTreeMap;

use gpui::{App, AppContext, Context, Entity, Global};

use super::file_drafts::{Drafts, FileDraft, QuitQuestion, quit_question};
use crate::workspaces::docs_rules::DocDraft;

#[derive(Default)]
pub struct DraftStore {
    files: Drafts,
    docs: BTreeMap<(String, String), DocDraft>,
}

struct GlobalDraftStore(Entity<DraftStore>);

impl Global for GlobalDraftStore {}

impl DraftStore {
    /// The store of the app; made the first time it is asked for.
    pub fn global(cx: &mut App) -> Entity<DraftStore> {
        if let Some(store) = cx.try_global::<GlobalDraftStore>() {
            return store.0.clone();
        }
        let store = cx.new(|_| DraftStore::default());
        cx.set_global(GlobalDraftStore(store.clone()));
        store
    }

    pub fn file(&self, task: &str, path: &str) -> Option<&FileDraft> {
        self.files.get(task, path)
    }

    /// None drops the draft.
    pub fn set_file(
        &mut self,
        task: &str,
        path: &str,
        draft: Option<FileDraft>,
        cx: &mut Context<Self>,
    ) {
        if self.files.get(task, path) != draft.as_ref() {
            self.files.set(task, path, draft);
            cx.notify();
        }
    }

    pub fn doc(&self, task: &str, name: &str) -> Option<&DocDraft> {
        self.docs.get(&(task.to_owned(), name.to_owned()))
    }

    /// None drops the draft.
    pub fn set_doc(
        &mut self,
        task: &str,
        name: &str,
        draft: Option<DocDraft>,
        cx: &mut Context<Self>,
    ) {
        let key = (task.to_owned(), name.to_owned());
        match draft {
            Some(draft) => self.docs.insert(key, draft),
            None => self.docs.remove(&key),
        };
        cx.notify();
    }

    /// What quitting would drop: files of the editor, and Docs edits that changed the text.
    pub fn quit_question(&self) -> Option<QuitQuestion> {
        let docs: Vec<(String, String)> = self
            .docs
            .iter()
            .filter(|(_, draft)| draft.is_dirty())
            .map(|(key, _)| key.clone())
            .collect();
        quit_question(&self.files.unsaved(), &docs)
    }
}
