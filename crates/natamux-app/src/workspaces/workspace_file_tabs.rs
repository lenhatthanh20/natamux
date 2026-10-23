//! The Docs tab and the editor tab of the open workspace (`DocsTab.tsx`, `FileEditor.tsx`): made
//! when they first show, kept while the same task (and, for the editor, the same file) is open.
//! Only the one that shows reads again when the workspace changes (`dirty_reads`); a hidden one
//! reads again when it shows.

use ui::prelude::*;

use super::WorkspaceView;
use crate::app::tab_rules::dirty_reads;
use crate::app::view_rules::WorkspaceTab;
use crate::files::file_editor::FileEditor;
use crate::workspaces::docs_tab::DocsTab;

impl WorkspaceView {
    pub(super) fn docs_body(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shows_again =
            self.file_tab_shown.replace(WorkspaceTab::Docs) != Some(WorkspaceTab::Docs);
        let docs = match &self.docs {
            Some(docs) if docs.read(cx).task_id() == task_id => {
                if shows_again {
                    docs.update(cx, |docs, cx| docs.refresh(cx));
                }
                docs.clone()
            }
            _ => {
                let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
                let docs = cx.new(|cx| DocsTab::new(dashboard, id, window, cx));
                self.docs = Some(docs.clone());
                docs
            }
        };
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
        let tab = self.dashboard.read(cx).shown_workspace_tab();
        let reads = dirty_reads(Some(tab), None);
        if let Some(docs) = self.docs.as_ref().filter(|_| reads.docs) {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
        if let Some(editor) = self.editor.as_ref().filter(|_| reads.editor) {
            editor.update(cx, |editor, cx| editor.refresh(cx));
        }
    }

    /// A logged change of the task can add a stage file: read while Docs shows.
    pub(super) fn refresh_docs(&mut self, cx: &mut Context<Self>) {
        let tab = self.dashboard.read(cx).shown_workspace_tab();
        if let Some(docs) = self
            .docs
            .as_ref()
            .filter(|_| dirty_reads(Some(tab), None).docs)
        {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
    }
}
