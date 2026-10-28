//! Docs and the open files over the terminals of the open workspace (`DocsTab.tsx`,
//! `FileEditor.tsx`): made when they first show, kept while the same task (and, for an editor,
//! its file) is open. Only the one that shows (in main, or the Docs list in the panel) reads
//! again when the workspace changes (`dirty_reads`); a hidden one reads again when it shows.

use ui::prelude::*;

use super::WorkspaceView;
use crate::app::tab_rules::{DirtyReads, dirty_reads};
use crate::app::view_rules::WorkspaceTab;
use crate::files::file_editor::FileEditor;
use crate::files::file_tab_strip::file_tab_strip;

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

    /// The tabs of the open files over the active one's editor (T-0220).
    pub(super) fn editor_body(
        &mut self,
        task_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let files = &self.dashboard.read(cx).files;
        let paths = files.editor_files(task_id).to_vec();
        let Some(path) = files.editor_file(task_id).map(String::from) else {
            return div().into_any_element();
        };
        let tab_shows_again =
            self.file_tab_shown.replace(WorkspaceTab::Editor) != Some(WorkspaceTab::Editor);
        let shows_again =
            self.editor_shown.replace(path.clone()).as_ref() != Some(&path) || tab_shows_again;
        self.editors.retain(|editor| {
            let editor = editor.read(cx);
            editor.task_id() == task_id && paths.iter().any(|open| open == editor.path())
        });
        let made = self
            .editors
            .iter()
            .find(|editor| editor.read(cx).path() == path)
            .cloned();
        let editor = match made {
            Some(editor) => {
                if shows_again {
                    editor.update(cx, |editor, cx| editor.refresh(cx));
                }
                editor
            }
            None => {
                let (dashboard, id) = (self.dashboard.clone(), task_id.to_owned());
                let editor = cx.new(|cx| FileEditor::new(dashboard, id, path, window, cx));
                self.editors.push(editor.clone());
                editor
            }
        };
        v_flex()
            .size_full()
            .child(file_tab_strip(&self.dashboard, task_id, cx))
            .child(v_flex().flex_1().min_h_0().child(editor))
            .into_any_element()
    }

    /// `workspace.dirty`, the fallback poll: the stage files or the open file, if it shows.
    pub(super) fn refresh_file_tabs(&mut self, cx: &mut Context<Self>) {
        let reads = self.dirty_reads_now(cx);
        if let Some(docs) = self.docs.as_ref().filter(|_| reads.docs) {
            docs.update(cx, |docs, cx| docs.refresh(cx));
        }
        if reads.editor {
            let dashboard = self.dashboard.read(cx);
            let active = self.editors.iter().find(|editor| {
                let editor = editor.read(cx);
                dashboard.files.editor_file(editor.task_id()) == Some(editor.path())
            });
            if let Some(editor) = active.cloned() {
                editor.update(cx, |editor, cx| editor.refresh(cx));
            }
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
