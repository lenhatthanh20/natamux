//! The small changes of the view state that the app makes as the user works.

use super::{
    DETAILS_MAX, DETAILS_MIN, FILES_MAX, FILES_MIN, SIDEBAR_MAX, SIDEBAR_MIN, ViewState,
    with_section,
};
use crate::app::view_rules::{DetailsView, WorkspaceTab, with_maximized, with_tab, with_terminal};

impl ViewState {
    /// The branch a workspace of this repo started from the last time (`lastBase`).
    pub fn last_base(&self, repo: &str) -> Option<&str> {
        self.last_base
            .iter()
            .find(|(each, _)| each == repo)
            .map(|(_, base)| base.as_str())
    }

    /// `rememberBase`: the newest choice is the last entry.
    pub fn remember_base(&mut self, repo: &str, base: &str) {
        self.last_base.retain(|(each, _)| each != repo);
        self.last_base.push((repo.to_owned(), base.to_owned()));
    }

    pub fn toggle_blurred(&mut self, repo: &str) {
        if self.blurred.iter().any(|each| each == repo) {
            self.blurred.retain(|each| each != repo);
        } else {
            self.blurred.push(repo.to_owned());
        }
    }

    pub fn set_sidebar_width(&mut self, width: f32) {
        self.sidebar_width = width.clamp(SIDEBAR_MIN, SIDEBAR_MAX);
    }

    pub fn set_details_width(&mut self, width: f32) {
        self.details_width = width.clamp(DETAILS_MIN, DETAILS_MAX);
    }

    /// `showProgress`: the details dock open on Progress.
    pub fn show_progress(&mut self) {
        self.details_view = Some(DetailsView::Progress);
        self.details_open = true;
    }

    /// `showFiles`: the details dock open on Files.
    pub fn show_files(&mut self) {
        self.details_view = Some(DetailsView::Files);
        self.details_open = true;
    }

    /// `rememberTab`: Editor needs an open file, so it is not kept.
    pub fn remember_tab(&mut self, task_id: &str, tab: WorkspaceTab) {
        if WorkspaceTab::parse(tab.name()).is_some() {
            self.tabs = with_tab(&self.tabs, task_id, tab);
        }
    }

    pub fn tab_of(&self, task_id: &str) -> Option<WorkspaceTab> {
        self.tabs
            .iter()
            .find(|(id, _)| id == task_id)
            .map(|(_, tab)| *tab)
    }

    pub fn is_maximized(&self, task_id: &str) -> bool {
        self.maximized.iter().any(|id| id == task_id)
    }

    pub fn set_maximized(&mut self, task_id: &str, on: bool) {
        self.maximized = with_maximized(&self.maximized, task_id, on);
    }

    pub fn remember_terminal(&mut self, task_id: &str, name: &str) {
        self.terminals = with_terminal(&self.terminals, task_id, name);
    }

    pub fn terminal_of(&self, task_id: &str) -> Option<&str> {
        self.terminals
            .iter()
            .find(|(id, _)| id == task_id)
            .map(|(_, name)| name.as_str())
    }

    pub fn set_files_width(&mut self, width: u32) {
        self.files_width = width.clamp(FILES_MIN, FILES_MAX);
    }

    /// Whether a section of the details panel is open; `default` until the user folds it.
    pub fn section_open(&self, id: &str, default: bool) -> bool {
        self.sections
            .iter()
            .find(|(each, _)| each == id)
            .map_or(default, |(_, open)| *open)
    }

    pub fn set_section(&mut self, id: &str, open: bool) {
        self.sections = with_section(&self.sections, id, open);
    }
}
