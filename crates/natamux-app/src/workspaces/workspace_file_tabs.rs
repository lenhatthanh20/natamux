//! Docs and the open file over the terminals of the open workspace (`DocsTab.tsx`,
//! `FileEditor.tsx`): made when they first show, kept while the same task (and, for the editor,
//! the same file) is open. Only the one that shows (in main, or the Docs list in the panel) reads
//! again when the workspace changes (`dirty_reads`); a hidden one reads again when it shows.

use ui::prelude::*;

use super::WorkspaceView;
use crate::app::tab_rules::{DirtyReads, dirty_reads};
use crate::app::view_rules::WorkspaceTab;
use crate::files::file_editor::FileEditor;

impl WorkspaceView {
    pub(super) fn docs_body(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shows_again =
            self.file_tab_shown.replace(WorkspaceTab::Docs) != Some(WorkspaceTab::Docs);
        let made = self
            .docs
            .as_ref()
            .is_some_and(|docs| docs.read(cx).task_id() == task_id);
        let docs = self.docs_of(task_id, window, cx);
        if made && shows_again {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
        let picked = self
            .dashboard
            .update(cx, |dashboard, _| dashboard.take_doc_request(task_id));
        if let Some(name) = picked {
            docs.update(cx, |docs, cx| docs.pick(name, cx));
        }
        docs.into_any_element()
    }

    pub(super) fn editor_body(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(path) = self
            .dashboard
            .read(cx)
            .files
            .editor_file(task_id)
            .map(String::from)
        else {
            return div().into_any_element();
        };
        let shows_again =
            self.file_tab_shown.replace(WorkspaceTab::Editor) != Some(WorkspaceTab::Editor);
        let editor = match &self.editor {
            Some(editor)
                if editor.read(cx).task_id() == task_id && editor.read(cx).path() == path =>
            {
                if shows_again {
                    editor.update(cx, |editor, cx| editor.refresh(cx));
                }
                editor.clone()
            }
            _ => {
                let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
                let editor = cx.new(|cx| FileEditor::new(dashboard, id, path, window, cx));
                self.editor = Some(editor.clone());
                editor
            }
        };
        editor.into_any_element()
    }

    /// `workspace.dirty`, the fallback poll: the stage files or the open file, if it shows.
    pub(super) fn refresh_file_tabs(&mut self, cx: &mut Context<Self>) {
        let reads = self.dirty_reads_now(cx);
        if let Some(docs) = self.docs.as_ref().filter(|_| reads.docs) {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
        if let Some(editor) = self.editor.as_ref().filter(|_| reads.editor) {
            editor.update(cx, |editor, cx| editor.refresh(cx));
        }
    }

    /// A logged change of the task can add a stage file: read while Docs shows.
    pub(super) fn refresh_docs(&mut self, cx: &mut Context<Self>) {
        let docs_shown = self.dirty_reads_now(cx).docs;
        if let Some(docs) = self.docs.as_ref().filter(|_| docs_shown) {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
    }

    /// What main and the panel show now, for `dirty_reads`.
    fn dirty_reads_now(&self, cx: &App) -> DirtyReads {
        let dashboard = self.dashboard.read(cx);
        let details = dashboard
            .details_shown()
            .then(|| dashboard.details_view_now());
        dirty_reads(Some(dashboard.shown_workspace_tab()), details)
    }
}
