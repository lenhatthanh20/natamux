//! What main shows for the open workspace and which view the details panel shows
//! (`WorkspaceTab` and `DetailsView` of `lib/view-store/helpers.ts`), as pure rules. Since T-0183
//! main has no tab bar: it shows the terminals, or Changes, Docs or the open file over them; the
//! panel's views stand on a rail at the right edge.

/// What main shows. `editor` only while a file of the worktree is open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceTab {
    Changes,
    Terminal,
    Docs,
    Editor,
}

impl WorkspaceTab {
    pub fn name(self) -> &'static str {
        match self {
            WorkspaceTab::Changes => "changes",
            WorkspaceTab::Terminal => "terminal",
            WorkspaceTab::Docs => "docs",
            WorkspaceTab::Editor => "editor",
        }
    }

    /// Only the screens the app remembers read back: Editor needs an open file, and a saved
    /// "history" (before T-0183) gives None, so the workspace opens on its terminals.
    pub fn parse(text: &str) -> Option<WorkspaceTab> {
        [
            WorkspaceTab::Terminal,
            WorkspaceTab::Changes,
            WorkspaceTab::Docs,
        ]
        .into_iter()
        .find(|tab| tab.name() == text)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetailsView {
    Progress,
    Changes,
    History,
    Docs,
    Files,
    Details,
}

/// Every view, in the rail's order, top to bottom.
const ALL_VIEWS: [DetailsView; 6] = [
    DetailsView::Progress,
    DetailsView::Changes,
    DetailsView::History,
    DetailsView::Docs,
    DetailsView::Files,
    DetailsView::Details,
];

impl DetailsView {
    /// The names of `natamux.details-view`.
    pub fn name(self) -> &'static str {
        match self {
            DetailsView::Progress => "progress",
            DetailsView::Changes => "changes",
            DetailsView::History => "history",
            DetailsView::Docs => "docs",
            DetailsView::Files => "files",
            DetailsView::Details => "details",
        }
    }

    pub fn parse(text: &str) -> Option<DetailsView> {
        ALL_VIEWS.into_iter().find(|view| view.name() == text)
    }

    /// Changes, History and Docs show the open workspace's git and stage files.
    fn needs_open_workspace(self) -> bool {
        matches!(
            self,
            DetailsView::Changes | DetailsView::History | DetailsView::Docs
        )
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

/// The views of the rail, top to bottom. Files needs a workspace; Changes, History and Docs
/// need the panel beside the open workspace (`beside_workspace`), as they cover its main.
pub fn details_tabs(has_workspace: bool, beside_workspace: bool) -> Vec<DetailsView> {
    ALL_VIEWS
        .into_iter()
        .filter(|view| match view {
            DetailsView::Files => has_workspace,
            view if view.needs_open_workspace() => beside_workspace,
            _ => true,
        })
        .collect()
}

/// A remembered view the task cannot show (Files with no workspace, Changes beside the list)
/// shows Progress.
pub fn shown_details_view(
    view: DetailsView,
    has_workspace: bool,
    beside_workspace: bool,
) -> DetailsView {
    if details_tabs(has_workspace, beside_workspace).contains(&view) {
        view
    } else {
        DetailsView::Progress
    }
}

/// What main shows after the rail (or a key) picks `view`: Changes and Docs cover the
/// terminals; the other views leave main as it is.
pub fn main_after_view(view: DetailsView, shown: WorkspaceTab) -> WorkspaceTab {
    match view {
        DetailsView::Changes => WorkspaceTab::Changes,
        DetailsView::Docs => WorkspaceTab::Docs,
        _ => shown,
    }
}

/// A click on a rail icon.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RailClick {
    Show(DetailsView),
    /// The picked view of an open panel: the panel closes, as a Zed or VS Code side bar.
    Close,
}

pub fn rail_click(view: DetailsView, picked: DetailsView, open: bool) -> RailClick {
    if open && view == picked {
        RailClick::Close
    } else {
        RailClick::Show(view)
    }
}

/// Esc over the open workspace: while Changes, Docs or the open file covers the terminals, it
/// shows them again, before it would close the task (`Root::back`).
pub fn esc_shows_terminals(task_open: bool, shown: WorkspaceTab) -> bool {
    task_open && shown != WorkspaceTab::Terminal
}

/// The screen shown: the editor is nothing without its file.
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
        // The list of the stage files is in the panel; the picked file covers main.
        docs: workspace_tab == Some(WorkspaceTab::Docs) || details == Some(DetailsView::Docs),
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_back_every_screen_but_the_editor_and_a_saved_history() {
        for tab in [
            WorkspaceTab::Terminal,
            WorkspaceTab::Changes,
            WorkspaceTab::Docs,
        ] {
            assert_eq!(WorkspaceTab::parse(tab.name()), Some(tab));
        }
        assert_eq!(WorkspaceTab::parse("editor"), None);
        // History moved to the panel (T-0183): a saved one opens on the terminals.
        assert_eq!(WorkspaceTab::parse("history"), None);
        assert_eq!(WorkspaceTab::parse("progress"), None);
    }

    #[test]
    fn reads_back_every_view_of_the_panel() {
        for view in ALL_VIEWS {
            assert_eq!(DetailsView::parse(view.name()), Some(view));
        }
        assert_eq!(DetailsView::parse("terminal"), None);
    }

    #[test]
    fn shows_progress_for_a_remembered_view_the_task_cannot_show() {
        use DetailsView::*;
        assert_eq!(shown_details_view(Files, false, false), Progress);
        assert_eq!(shown_details_view(Files, true, false), Files);
        assert_eq!(shown_details_view(Details, false, false), Details);
        // Changes, History and Docs beside the list: Progress.
        for view in [Changes, History, Docs] {
            assert_eq!(shown_details_view(view, true, false), Progress);
            assert_eq!(shown_details_view(view, true, true), view);
        }
    }

    #[test]
    fn puts_the_views_on_the_rail_top_to_bottom() {
        use DetailsView::*;
        assert_eq!(
            details_tabs(true, true),
            vec![Progress, Changes, History, Docs, Files, Details]
        );
        // A task selected in the list: no Changes, History or Docs.
        assert_eq!(details_tabs(true, false), vec![Progress, Files, Details]);
        assert_eq!(details_tabs(false, false), vec![Progress, Details]);
    }

    #[test]
    fn changes_and_docs_cover_the_terminals_the_other_views_leave_main() {
        use WorkspaceTab::*;
        assert_eq!(main_after_view(DetailsView::Changes, Terminal), Changes);
        assert_eq!(main_after_view(DetailsView::Docs, Editor), Docs);
        for view in [
            DetailsView::Progress,
            DetailsView::History,
            DetailsView::Files,
            DetailsView::Details,
        ] {
            assert_eq!(main_after_view(view, Terminal), Terminal);
            assert_eq!(main_after_view(view, Changes), Changes);
        }
    }

    #[test]
    fn esc_shows_the_terminals_while_a_screen_covers_them() {
        use WorkspaceTab::*;
        for covered in [Changes, Docs, Editor] {
            assert!(esc_shows_terminals(true, covered));
        }
        // The terminals already show: Esc goes on to close the task.
        assert!(!esc_shows_terminals(true, Terminal));
        // No task open: nothing covers main.
        assert!(!esc_shows_terminals(false, Changes));
    }

    #[test]
    fn a_click_on_the_picked_view_closes_the_open_panel() {
        use DetailsView::*;
        assert_eq!(rail_click(Changes, Changes, true), RailClick::Close);
        assert_eq!(
            rail_click(Changes, Changes, false),
            RailClick::Show(Changes)
        );
        assert_eq!(rail_click(History, Changes, true), RailClick::Show(History));
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
        // The Docs list in the panel reads the stage files while the terminals show.
        assert!(dirty_reads(Some(WorkspaceTab::Terminal), Some(DetailsView::Docs)).docs);
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
