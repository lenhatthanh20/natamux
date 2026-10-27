//! What the web keeps per browser (`natamux.*` in localStorage), kept in one file: `view.json`
//! in the app's data folder. The rules of each part are `app/view_rules.rs` and
//! `sidebar/sidebar_tree.rs`; this file reads and writes them.

use std::path::{Path, PathBuf};

use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value, json};

use crate::app::layout_mode::{PANEL_WIDTH, SIDEBAR_WIDTH};
use crate::app::theme_catalog::ThemeNames;
use crate::app::view_rules::{
    DetailsView, Entries, MAX_RECENT, WorkspaceTab, parse_pinned, with_seen, with_tab,
};
use crate::git::change_tree::FileViewMode;
use crate::sidebar::sidebar_tree::{Folds, with_fold};
use crate::terminals::pane_rules::{PaneLayout, parse_pane_layout};

pub const FILE_NAME: &str = "view.json";
/// `useStoredWidth('natamux.sidebar-width', …, 220, 440)`; the default is `SIDEBAR_WIDTH`.
pub const SIDEBAR_MIN: f32 = 220.0;
pub const SIDEBAR_MAX: f32 = 440.0;
/// `useStoredWidth('natamux.details-width', PANEL_WIDTH, 320, 560)`.
pub const DETAILS_MIN: f32 = 320.0;
pub const DETAILS_MAX: f32 = 560.0;
/// The file list of the Changes tab: `useStoredWidth('natamux.files-width', 260, 200, 420)`.
pub const FILES_DEFAULT: u32 = 260;
pub const FILES_MIN: u32 = 200;
pub const FILES_MAX: u32 = 420;
/// Folded sections of the details panel kept at most (`natamux.section.<id>`).
const MAX_SECTIONS: usize = 50;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Table,
    Board,
    Progress,
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Table => "table",
            Layout::Board => "board",
            Layout::Progress => "progress",
        }
    }
}

/// The switches of the diff, with the defaults of the web.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffFlags {
    pub side_by_side: bool,
    pub collapse_unchanged: bool,
    pub ignore_whitespace: bool,
    pub show_moves: bool,
    pub wrap_prose: bool,
    pub wrap_code: bool,
}

impl Default for DiffFlags {
    fn default() -> Self {
        DiffFlags {
            side_by_side: true,
            collapse_unchanged: true,
            ignore_whitespace: true,
            show_moves: false,
            wrap_prose: true,
            wrap_code: false,
        }
    }
}

impl DiffFlags {
    fn parse(value: Option<&Value>) -> Self {
        let default = DiffFlags::default();
        let get = |name: &str, fallback: bool| flag(value.and_then(|map| map.get(name)), fallback);
        DiffFlags {
            side_by_side: get("sideBySide", default.side_by_side),
            collapse_unchanged: get("collapseUnchanged", default.collapse_unchanged),
            ignore_whitespace: get("ignoreWhitespace", default.ignore_whitespace),
            show_moves: get("showMoves", default.show_moves),
            wrap_prose: get("wrapProse", default.wrap_prose),
            wrap_code: get("wrapCode", default.wrap_code),
        }
    }

    fn to_json(self) -> Value {
        json!({
            "sideBySide": self.side_by_side,
            "collapseUnchanged": self.collapse_unchanged,
            "ignoreWhitespace": self.ignore_whitespace,
            "showMoves": self.show_moves,
            "wrapProse": self.wrap_prose,
            "wrapCode": self.wrap_code,
        })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ViewState {
    pub folds: Folds,
    /// Repo ids whose names are blurred.
    pub blurred: Vec<String>,
    pub pinned: Vec<String>,
    pub layout: Layout,
    pub sidebar_open: bool,
    pub sidebar_width: f32,
    pub show_finished: bool,
    pub seen: Entries<TaskState>,
    /// Repo id to the branch its last workspace started from.
    pub last_base: Vec<(String, String)>,
    /// The tasks of the Agents grid, in order. None: the default list (`agentPanes`).
    pub agent_panes: Option<Vec<String>>,
    pub pane_layout: PaneLayout,
    pub details_width: f32,
    /// The tab the user last picked in the details panel. None: the default of the place.
    pub details_view: Option<DetailsView>,
    /// Sections of the details panel by id, open or folded; a missing one has its default.
    pub sections: Vec<(String, bool)>,
    /// The details dock is open, for every task at once (one flag, as a Zed dock).
    pub details_open: bool,
    /// System notifications for tasks that start to need the user (off until switched on).
    pub notify: bool,
    /// The last workspace tab of each task (`tabByTask`): terminal, changes, history or docs.
    pub tabs: Entries<WorkspaceTab>,
    /// The Docs tab shows the raw text (`natamux.docs-source`).
    pub docs_source: bool,
    pub changes_view: FileViewMode,
    pub files_width: u32,
    pub diff: DiffFlags,
    /// `natamux.theme`: system, light or dark, read by `app/appearance_rules.rs`.
    pub theme: String,
    /// The theme of each look (`lightTheme`, `darkTheme`), from `app/theme_catalog.rs`.
    pub themes: ThemeNames,
    /// `natamux.ui-size`: compact, default or large.
    pub ui_size: String,
    /// `natamux.terminal-font`: `{ family?, size? }` as the user saved it, read by
    /// `terminals/terminal_font.rs`.
    pub terminal_font: Option<Value>,
    /// Task ids of the workspaces opened, newest first (`natamux.recent`).
    pub recent: Vec<String>,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            folds: Vec::new(),
            blurred: Vec::new(),
            pinned: Vec::new(),
            layout: Layout::Table,
            sidebar_open: true,
            sidebar_width: SIDEBAR_WIDTH,
            show_finished: false,
            seen: Vec::new(),
            last_base: Vec::new(),
            agent_panes: None,
            pane_layout: PaneLayout::default(),
            details_width: PANEL_WIDTH,
            details_view: None,
            sections: Vec::new(),
            details_open: true,
            notify: false,
            tabs: Vec::new(),
            docs_source: false,
            changes_view: FileViewMode::List,
            files_width: FILES_DEFAULT,
            diff: DiffFlags::default(),
            theme: "system".to_owned(),
            themes: ThemeNames::default(),
            ui_size: "default".to_owned(),
            terminal_font: None,
            recent: Vec::new(),
        }
    }
}

/// The last tab of each workspace, through `with_tab` so the cap holds.
fn tabs_of(pairs: Map<String, Value>) -> Entries<WorkspaceTab> {
    pairs
        .into_iter()
        .filter_map(|(id, tab)| Some((id, WorkspaceTab::parse(tab.as_str()?)?)))
        .fold(Vec::new(), |tabs, (id, tab)| with_tab(&tabs, &id, tab))
}

fn files_width_of(value: Option<&Value>, default: u32) -> u32 {
    value
        .and_then(Value::as_f64)
        .map_or(default, |width| (width as u32).clamp(FILES_MIN, FILES_MAX))
}

// Through the rules, so the caps hold for a file written by hand too.
fn folds_of(pairs: Map<String, Value>) -> Folds {
    pairs
        .into_iter()
        .filter_map(|(key, open)| open.as_bool().map(|open| (key, open)))
        .fold(Vec::new(), |folds, (key, open)| {
            with_fold(&folds, &key, open)
        })
}

fn seen_of(pairs: Map<String, Value>) -> Entries<TaskState> {
    let seen: Vec<(String, TaskState)> = pairs
        .into_iter()
        .filter_map(|(id, state)| Some((id, serde_json::from_value(state).ok()?)))
        .collect();
    let refs: Vec<(&str, TaskState)> = seen
        .iter()
        .map(|(id, state)| (id.as_str(), *state))
        .collect();
    with_seen(&[], &refs, true)
}

fn layout_of(value: Option<&Value>) -> Layout {
    match value.and_then(Value::as_str) {
        Some("board") => Layout::Board,
        Some("progress") => Layout::Progress,
        _ => Layout::Table,
    }
}

fn panel_width_of(value: Option<&Value>, default: f32, min: f32, max: f32) -> f32 {
    value
        .and_then(Value::as_f64)
        .map_or(default, |width| (width as f32).clamp(min, max))
}

impl ViewState {
    /// Each part that cannot be read falls back to its default; text that is no JSON object is
    /// the default view.
    pub fn parse(text: &str) -> ViewState {
        let Ok(Value::Object(map)) = serde_json::from_str::<Value>(text) else {
            return ViewState::default();
        };
        let default = ViewState::default();
        let pairs = |name: &str| match map.get(name) {
            Some(Value::Object(entries)) => entries.clone(),
            _ => Map::new(),
        };
        ViewState {
            folds: folds_of(pairs("folds")),
            blurred: strings(map.get("blurred")),
            pinned: strings(map.get("pinned")),
            layout: layout_of(map.get("layout")),
            sidebar_open: flag(map.get("sidebarOpen"), default.sidebar_open),
            sidebar_width: panel_width_of(
                map.get("sidebarWidth"),
                default.sidebar_width,
                SIDEBAR_MIN,
                SIDEBAR_MAX,
            ),
            show_finished: flag(map.get("showFinished"), default.show_finished),
            seen: seen_of(pairs("seen")),
            last_base: pairs("lastBase")
                .into_iter()
                .filter_map(|(repo, base)| Some((repo, base.as_str()?.to_owned())))
                .collect(),
            agent_panes: match map.get("agentPanes") {
                Some(Value::Array(_)) => Some(strings(map.get("agentPanes"))),
                _ => None,
            },
            pane_layout: parse_pane_layout(map.get("paneLayout").and_then(Value::as_str)),
            details_width: panel_width_of(
                map.get("detailsWidth"),
                default.details_width,
                DETAILS_MIN,
                DETAILS_MAX,
            ),
            details_view: map
                .get("detailsView")
                .and_then(Value::as_str)
                .and_then(DetailsView::parse),
            sections: pairs("sections")
                .into_iter()
                .filter_map(|(id, open)| Some((id, open.as_bool()?)))
                .fold(Vec::new(), |sections, (id, open)| {
                    with_section(&sections, &id, open)
                }),
            details_open: flag(map.get("detailsOpen"), default.details_open),
            notify: flag(map.get("notify"), default.notify),
            tabs: tabs_of(pairs("tabs")),
            docs_source: flag(map.get("docsSource"), default.docs_source),
            changes_view: map
                .get("changesView")
                .and_then(Value::as_str)
                .map_or(default.changes_view, FileViewMode::parse),
            files_width: files_width_of(map.get("filesWidth"), default.files_width),
            diff: DiffFlags::parse(map.get("diff")),
            theme: text_or(map.get("theme"), &default.theme),
            themes: ThemeNames::parse(map.get("lightTheme"), map.get("darkTheme")),
            ui_size: text_or(map.get("uiSize"), &default.ui_size),
            terminal_font: map
                .get("terminalFont")
                .filter(|font| font.is_object())
                .cloned(),
            recent: strings(map.get("recent"))
                .into_iter()
                .take(MAX_RECENT)
                .collect(),
        }
    }

    pub fn to_json(&self) -> String {
        let folds: Map<String, Value> = self
            .folds
            .iter()
            .map(|(key, open)| (key.clone(), Value::Bool(*open)))
            .collect();
        let seen: Map<String, Value> = self
            .seen
            .iter()
            .map(|(id, state)| (id.clone(), Value::from(state.name())))
            .collect();
        let last_base: Map<String, Value> = self
            .last_base
            .iter()
            .map(|(repo, base)| (repo.clone(), Value::from(base.as_str())))
            .collect();
        let value = json!({
            "folds": folds,
            "blurred": self.blurred,
            "pinned": self.pinned,
            "layout": self.layout.name(),
            "sidebarOpen": self.sidebar_open,
            "sidebarWidth": self.sidebar_width,
            "showFinished": self.show_finished,
            "seen": seen,
            "lastBase": last_base,
            "agentPanes": self.agent_panes,
            "paneLayout": self.pane_layout.as_str(),
            "detailsWidth": self.details_width,
            "detailsView": self.details_view.map(DetailsView::name),
            "sections": self
                .sections
                .iter()
                .map(|(id, open)| (id.clone(), Value::Bool(*open)))
                .collect::<Map<String, Value>>(),
            "detailsOpen": self.details_open,
            "notify": self.notify,
            "tabs": self
                .tabs
                .iter()
                .map(|(id, tab)| (id.clone(), Value::from(tab.name())))
                .collect::<Map<String, Value>>(),
            "docsSource": self.docs_source,
            "changesView": self.changes_view.name(),
            "filesWidth": self.files_width,
            "diff": self.diff.to_json(),
            "theme": self.theme,
            "lightTheme": self.themes.light,
            "darkTheme": self.themes.dark,
            "uiSize": self.ui_size,
            "terminalFont": self.terminal_font,
            "recent": self.recent,
        });
        format!("{value:#}\n")
    }

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

    pub fn set_details_width(&mut self, width: f32) {
        self.details_width = width.clamp(DETAILS_MIN, DETAILS_MAX);
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

/// The newest choice last; the oldest go when there are too many.
fn with_section(current: &[(String, bool)], id: &str, open: bool) -> Vec<(String, bool)> {
    let mut next: Vec<(String, bool)> = current
        .iter()
        .filter(|(each, _)| each != id)
        .cloned()
        .collect();
    next.push((id.to_owned(), open));
    let extra = next.len().saturating_sub(MAX_SECTIONS);
    next.drain(..extra);
    next
}

fn strings(value: Option<&Value>) -> Vec<String> {
    parse_pinned(value.map(Value::to_string).as_deref())
}

fn text_or(value: Option<&Value>, fallback: &str) -> String {
    value.and_then(Value::as_str).unwrap_or(fallback).to_owned()
}

fn flag(value: Option<&Value>, fallback: bool) -> bool {
    value.and_then(Value::as_bool).unwrap_or(fallback)
}

/// A missing or broken file is the default view: it never stops the app, and the next save
/// replaces it.
pub fn load(folder: &Path) -> ViewState {
    std::fs::read_to_string(folder.join(FILE_NAME))
        .map(|text| ViewState::parse(&text))
        .unwrap_or_default()
}

/// Writes a new file and renames it over the old one, so a crash never leaves half a file.
pub fn save(folder: &Path, state: &ViewState) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let partial = folder.join(format!("{FILE_NAME}.partial"));
    std::fs::write(&partial, state.to_json())?;
    std::fs::rename(&partial, folder.join(FILE_NAME))
}

/// The app's data folder on this OS, from the environment.
pub fn folder() -> Option<PathBuf> {
    folder_for(std::env::consts::OS, |name| std::env::var_os(name))
}

/// `os` is `std::env::consts::OS`. Linux and the rest follow the XDG data folder.
pub fn folder_for(os: &str, var: impl Fn(&str) -> Option<std::ffi::OsString>) -> Option<PathBuf> {
    let set = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match os {
        "macos" => Some(set("HOME")?.join("Library/Application Support/Natamux")),
        "windows" => Some(set("APPDATA")?.join("Natamux")),
        _ => Some(
            set("XDG_CONFIG_HOME")
                .or_else(|| Some(set("HOME")?.join(".config")))?
                .join("Natamux"),
        ),
    }
}

#[cfg(test)]
#[path = "view_state_tests.rs"]
mod tests;
