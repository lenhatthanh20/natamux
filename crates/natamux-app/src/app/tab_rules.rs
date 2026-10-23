//! The tabs of the workspace and of the details panel (`WorkspaceTab` and `DetailsView` of
//! `lib/view-store/helpers.ts`), as pure rules.

/// `editor` shows only while a file of the worktree is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    Changes,
    History,
    Terminal,
    Docs,
    Editor,
}

impl WorkspaceTab {
    pub fn name(self) -> &'static str {
        match self {
            WorkspaceTab::Changes => "changes",
            WorkspaceTab::History => "history",
            WorkspaceTab::Terminal => "terminal",
            WorkspaceTab::Docs => "docs",
            WorkspaceTab::Editor => "editor",
        }
    }

    /// Only the tabs the app remembers read back: Editor needs an open file.
    pub fn parse(text: &str) -> Option<WorkspaceTab> {
        [
            WorkspaceTab::Terminal,
            WorkspaceTab::Changes,
            WorkspaceTab::History,
            WorkspaceTab::Docs,
        ]
        .into_iter()
        .find(|tab| tab.name() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsView {
    Progress,
    Files,
    Details,
}

impl DetailsView {
    /// The names of `natamux.details-view`.
    pub fn name(self) -> &'static str {
        match self {
            DetailsView::Progress => "progress",
            DetailsView::Files => "files",
            DetailsView::Details => "details",
        }
    }

    pub fn parse(text: &str) -> Option<DetailsView> {
        [
            DetailsView::Progress,
            DetailsView::Files,
            DetailsView::Details,
        ]
        .into_iter()
        .find(|view| view.name() == text)
    }
}

/// Until the user picks a view, the details show the progress beside a workspace and the task
/// itself beside the list.
pub fn details_view_for(picked: Option<DetailsView>, in_workspace: bool) -> DetailsView {
    picked.unwrap_or(if in_workspace {
        DetailsView::Progress
    } else {
        DetailsView::Details
    })
}

/// The Files tab is there only for a task with a workspace; a remembered Files shows Progress
/// for any other task.
pub fn shown_details_view(view: DetailsView, has_workspace: bool) -> DetailsView {
    if view == DetailsView::Files && !has_workspace {
        DetailsView::Progress
    } else {
        view
    }
}

/// The tabs of the details panel, in order.
pub fn details_tabs(has_workspace: bool) -> Vec<DetailsView> {
    if has_workspace {
        vec![
            DetailsView::Progress,
            DetailsView::Files,
            DetailsView::Details,
        ]
    } else {
        vec![DetailsView::Progress, DetailsView::Details]
    }
}

/// The tab shown: the editor tab is nothing without its file.
pub fn shown_tab(picked: WorkspaceTab, file_open: bool) -> WorkspaceTab {
    if picked == WorkspaceTab::Editor && !file_open {
        WorkspaceTab::Terminal
    } else {
        picked
    }
}

/// What `workspace.dirty` and the 60 s fallback poll read again (`WORKSPACE_KEYS` of
/// `lib/live.ts`, which reads only the queries on screen): the git state while the workspace is
/// open, and each screen-6 view only while it shows. The shells are not read again.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DirtyReads {
    pub git: bool,
    pub tree: bool,
    pub docs: bool,
    pub editor: bool,
}

/// `workspace_tab`: the tab the open workspace shows (None: no workspace is open). `details`:
/// the tab the details panel shows (None: the panel is closed).
pub fn dirty_reads(
    workspace_tab: Option<WorkspaceTab>,
    details: Option<DetailsView>,
) -> DirtyReads {
    DirtyReads {
        git: workspace_tab.is_some(),
        tree: details == Some(DetailsView::Files),
        docs: workspace_tab == Some(WorkspaceTab::Docs),
        editor: workspace_tab == Some(WorkspaceTab::Editor),
    }
}

/// Whether a view showed when last drawn, so it reads again when it shows after it was hidden:
/// by another tab, or by its panel that closed (a closed panel is not drawn, so it hides the view
/// when it closes).
#[derive(Clone, Copy, Debug, Default)]
pub struct ShowWatch {
    shown: bool,
}

impl ShowWatch {
    /// The view draws. True when it was hidden: it reads again.
    pub fn draw(&mut self) -> bool {
        !std::mem::replace(&mut self.shown, true)
    }

    pub fn hide(&mut self) {
        self.shown = false;
    }
}

/// The files open in the editor tab and the stage file the Docs tab should pick, while the app
/// runs (`editorFileByTask`, `openFile`, `closeFile` and the `open-doc` event of the web).
#[derive(Default)]
pub struct FileViews {
    editor: Vec<(String, String)>,
    doc_request: Option<(String, String)>,
}

impl FileViews {
    pub fn editor_file(&self, task_id: &str) -> Option<&str> {
        self.editor
            .iter()
            .find(|(id, _)| id == task_id)
            .map(|(_, path)| path.as_str())
    }

    /// `openFile`: the file is the task's open file, and its workspace shows the editor tab.
    pub fn open(&mut self, task_id: &str, path: &str) -> WorkspaceTab {
        self.editor.retain(|(id, _)| id != task_id);
        self.editor.push((task_id.to_owned(), path.to_owned()));
        WorkspaceTab::Editor
    }

    /// `closeFile`: `shown` is the tab of the task's open workspace (None: it is not open), and
    /// `remembered` the tab the user picked last in it. Gives the tab to show instead, if any:
    /// the picked one, or the terminal.
    pub fn close(
        &mut self,
        task_id: &str,
        shown: Option<WorkspaceTab>,
        remembered: Option<WorkspaceTab>,
    ) -> Option<WorkspaceTab> {
        self.editor.retain(|(id, _)| id != task_id);
        (shown == Some(WorkspaceTab::Editor)).then(|| remembered.unwrap_or(WorkspaceTab::Terminal))
    }

    pub fn request_doc(&mut self, task_id: &str, name: &str) {
        self.doc_request = Some((task_id.to_owned(), name.to_owned()));
    }

    /// The Docs tab of the task takes its file once.
    pub fn take_doc_request(&mut self, task_id: &str) -> Option<String> {
        let ours = self
            .doc_request
            .as_ref()
            .is_some_and(|(task, _)| task == task_id);
        ours.then(|| self.doc_request.take().map(|(_, name)| name))
            .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_every_tab_but_the_editor() {
        for tab in [
            WorkspaceTab::Terminal,
            WorkspaceTab::Changes,
            WorkspaceTab::History,
            WorkspaceTab::Docs,
        ] {
            assert_eq!(WorkspaceTab::parse(tab.name()), Some(tab));
        }
        assert_eq!(WorkspaceTab::parse("editor"), None);
        assert_eq!(WorkspaceTab::parse("progress"), None);
    }

    #[test]
    fn shows_progress_for_a_remembered_files_tab_of_a_task_with_no_workspace() {
        assert_eq!(
            shown_details_view(DetailsView::Files, false),
            DetailsView::Progress
        );
        assert_eq!(
            shown_details_view(DetailsView::Files, true),
            DetailsView::Files
        );
        assert_eq!(
            shown_details_view(DetailsView::Details, false),
            DetailsView::Details
        );
    }

    #[test]
    fn offers_the_files_tab_only_with_a_workspace() {
        assert_eq!(
            details_tabs(true),
            vec![
                DetailsView::Progress,
                DetailsView::Files,
                DetailsView::Details
            ]
        );
        assert_eq!(
            details_tabs(false),
            vec![DetailsView::Progress, DetailsView::Details]
        );
    }

    #[test]
    fn shows_the_editor_tab_only_while_a_file_is_open() {
        assert_eq!(
            shown_tab(WorkspaceTab::Editor, false),
            WorkspaceTab::Terminal
        );
        assert_eq!(shown_tab(WorkspaceTab::Editor, true), WorkspaceTab::Editor);
        assert_eq!(shown_tab(WorkspaceTab::Docs, false), WorkspaceTab::Docs);
    }

    // The cases of 'the open file' (`view-store.test.ts`).
    #[test]
    fn opens_the_workspace_of_the_task_on_the_editor_tab() {
        let mut files = FileViews::default();
        assert_eq!(files.open("T-0004", "src/a.ts"), WorkspaceTab::Editor);
        assert_eq!(files.editor_file("T-0004"), Some("src/a.ts"));
        files.open("T-0004", "src/b.ts");
        assert_eq!(files.editor_file("T-0004"), Some("src/b.ts"));
        assert_eq!(files.editor_file("T-0005"), None);
    }

    #[test]
    fn closes_back_to_the_tab_the_user_picked_last_or_the_terminal() {
        let mut files = FileViews::default();
        files.open("T-0004", "src/a.ts");
        let back = files.close(
            "T-0004",
            Some(WorkspaceTab::Editor),
            Some(WorkspaceTab::History),
        );
        assert_eq!(back, Some(WorkspaceTab::History));
        assert_eq!(files.editor_file("T-0004"), None);

        files.open("T-0005", "b.ts");
        let back = files.close("T-0005", Some(WorkspaceTab::Editor), None);
        assert_eq!(back, Some(WorkspaceTab::Terminal));
        // Another tab shown, or another workspace open: the tab stays.
        files.open("T-0006", "c.ts");
        assert_eq!(files.close("T-0006", Some(WorkspaceTab::Docs), None), None);
        assert_eq!(files.close("T-0006", None, None), None);
    }

    #[test]
    fn gives_the_picked_stage_file_to_its_task_once() {
        let mut files = FileViews::default();
        files.request_doc("T-0001", "spec.md");
        assert_eq!(files.take_doc_request("T-0002"), None);
        assert_eq!(files.take_doc_request("T-0001"), Some("spec.md".into()));
        assert_eq!(files.take_doc_request("T-0001"), None);
    }

    #[test]
    fn reads_the_files_and_the_sizes_of_a_changed_workspace_again_not_its_shells() {
        let on_docs = dirty_reads(Some(WorkspaceTab::Docs), Some(DetailsView::Files));
        assert_eq!(
            on_docs,
            DirtyReads {
                git: true,
                tree: true,
                docs: true,
                editor: false
            }
        );
        // The terminal tab and the Progress tab: only the git state, no hidden view.
        let on_terminal = dirty_reads(Some(WorkspaceTab::Terminal), Some(DetailsView::Progress));
        assert_eq!(
            on_terminal,
            DirtyReads {
                git: true,
                ..DirtyReads::default()
            }
        );
        assert!(dirty_reads(Some(WorkspaceTab::Editor), None).editor);
        assert_eq!(dirty_reads(None, None), DirtyReads::default());
    }

    #[test]
    fn reads_the_tree_again_when_a_closed_panel_opens_on_files() {
        let mut tree = ShowWatch::default();
        assert!(tree.draw());
        assert!(!tree.draw());
        // The panel closes on Files, and opens again on Files.
        tree.hide();
        assert!(tree.draw());
        // Another tab, then Files again.
        tree.hide();
        tree.hide();
        assert!(tree.draw());
        assert!(!tree.draw());
    }
}
