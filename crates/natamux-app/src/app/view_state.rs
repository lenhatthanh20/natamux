//! What the web keeps per browser (`natamux.*` in localStorage), kept in one file: `view.json`
//! in the app's data folder. The rules of each part are `app/view_rules.rs` and
//! `sidebar/sidebar_tree.rs`; this file reads and writes them.

use std::path::{Path, PathBuf};

use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value, json};

use crate::app::appearance_rules::clamp_zoom_level;
use crate::app::layout_mode::{DETAILS_WIDTH, SIDEBAR_WIDTH};
use crate::app::theme_catalog::ThemeNames;
use crate::app::view_rules::{
    DetailsView, Entries, MAX_RECENT, WorkspaceTab, parse_pinned, with_maximized, with_seen,
    with_tab, with_terminal,
};
use crate::git::change_tree::FileViewMode;
use crate::sidebar::sidebar_tree::{Folds, with_fold};
use crate::terminals::pane_rules::{PaneLayout, parse_pane_layout};

#[path = "view_parts.rs"]
mod parts;
use parts::flag;
pub use parts::{DiffFlags, Layout};

pub const FILE_NAME: &str = "view.json";
/// The sidebar (`sidebarWidth`); the default is `SIDEBAR_WIDTH`.
pub const SIDEBAR_MIN: f32 = 280.0;
pub const SIDEBAR_MAX: f32 = 480.0;
/// The docked details panel without its rail (`detailsWidth`), resized on its own (T-0239).
/// The default is `DETAILS_WIDTH`: the sidebar minus the rail.
pub const DETAILS_MIN: f32 = 280.0;
pub const DETAILS_MAX: f32 = 560.0;
/// The file list of the Changes tab: `useStoredWidth('natamux.files-width', 260, 200, 420)`.
pub const FILES_DEFAULT: u32 = 260;
pub const FILES_MIN: u32 = 200;
pub const FILES_MAX: u32 = 420;
/// Folded sections of the details panel kept at most (`natamux.section.<id>`).
const MAX_SECTIONS: usize = 50;

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
    /// The sidebar shows each task's id before its title (off until switched on, T-0240).
    pub sidebar_ids: bool,
    pub seen: Entries<TaskState>,
    /// The background result of each task the user has seen (`view_rules::seen_results`).
    pub seen_results: Entries<String>,
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
    /// System notifications for tasks that start to need the user (on by default).
    pub notify: bool,
    /// Notifications also while the window is in front; off, the open window is enough.
    pub notify_always: bool,
    /// A note when a turn that background work started ends (off by default, T-0390).
    pub notify_background_results: bool,
    /// A task that waits for background work with no sign of life for this many minutes needs
    /// the user (`tasks/task_quiet.rs`); 0 is off.
    pub flag_background_after_minutes: u32,
    /// The last workspace tab of each task (`tabByTask`): terminal, changes, history or docs.
    pub tabs: Entries<WorkspaceTab>,
    /// Tasks whose terminals show one at a time, full size, until the user restores them.
    pub maximized: Vec<String>,
    /// The terminal each maximized task showed last.
    pub terminals: Entries<String>,
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
    /// `zoomLevel`: the zoom keys' steps on top of the UI size (`appearance_rules::zoom_factor`).
    pub zoom_level: i32,
    /// `natamux.terminal-font`: `{ family?, size? }` as the user saved it, read by
    /// `terminals/terminal_font.rs`.
    pub terminal_font: Option<Value>,
    /// Task ids of the workspaces opened, newest first (`natamux.recent`).
    pub recent: Vec<String>,
    /// The version whose release notes the app last showed or skipped (`whatsNewSeen`, T-0301).
    pub whats_new_seen: Option<String>,
    /// The editor tab trims trailing whitespace and ends the file with one newline on save.
    pub clean_on_save: bool,
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
            sidebar_ids: false,
            seen: Vec::new(),
            seen_results: Vec::new(),
            last_base: Vec::new(),
            agent_panes: None,
            pane_layout: PaneLayout::default(),
            details_width: DETAILS_WIDTH,
            details_view: None,
            sections: Vec::new(),
            details_open: true,
            notify: true,
            notify_always: false,
            notify_background_results: false,
            flag_background_after_minutes: 0,
            tabs: Vec::new(),
            maximized: Vec::new(),
            terminals: Vec::new(),
            docs_source: false,
            changes_view: FileViewMode::List,
            files_width: FILES_DEFAULT,
            diff: DiffFlags::default(),
            theme: "system".to_owned(),
            themes: ThemeNames::default(),
            ui_size: "default".to_owned(),
            zoom_level: 0,
            terminal_font: None,
            recent: Vec::new(),
            whats_new_seen: None,
            clean_on_save: false,
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
            sidebar_ids: flag(map.get("sidebarIds"), default.sidebar_ids),
            seen: seen_of(pairs("seen")),
            seen_results: pairs("seenResults")
                .into_iter()
                .filter_map(|(id, at)| Some((id, at.as_str()?.to_owned())))
                .collect(),
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
            notify_always: flag(map.get("notifyAlways"), default.notify_always),
            notify_background_results: flag(
                map.get("notifyBackgroundResults"),
                default.notify_background_results,
            ),
            flag_background_after_minutes: map
                .get("flagBackgroundAfterMinutes")
                .and_then(Value::as_u64)
                .and_then(|minutes| u32::try_from(minutes).ok())
                .unwrap_or(default.flag_background_after_minutes),
            tabs: tabs_of(pairs("tabs")),
            maximized: strings(map.get("maximized"))
                .iter()
                .fold(Vec::new(), |tasks, id| with_maximized(&tasks, id, true)),
            terminals: pairs("terminals")
                .into_iter()
                .filter_map(|(id, name)| Some((id, name.as_str()?.to_owned())))
                .fold(Vec::new(), |names, (id, name)| {
                    with_terminal(&names, &id, &name)
                }),
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
            zoom_level: zoom_level_of(map.get("zoomLevel")),
            terminal_font: map
                .get("terminalFont")
                .filter(|font| font.is_object())
                .cloned(),
            recent: strings(map.get("recent"))
                .into_iter()
                .take(MAX_RECENT)
                .collect(),
            whats_new_seen: map
                .get("whatsNewSeen")
                .and_then(Value::as_str)
                .map(str::to_owned),
            clean_on_save: flag(map.get("cleanOnSave"), default.clean_on_save),
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
            "sidebarIds": self.sidebar_ids,
            "seen": seen,
            "seenResults": self
                .seen_results
                .iter()
                .map(|(id, at)| (id.clone(), Value::from(at.as_str())))
                .collect::<Map<String, Value>>(),
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
            "notifyAlways": self.notify_always,
            "notifyBackgroundResults": self.notify_background_results,
            "flagBackgroundAfterMinutes": self.flag_background_after_minutes,
            "tabs": self
                .tabs
                .iter()
                .map(|(id, tab)| (id.clone(), Value::from(tab.name())))
                .collect::<Map<String, Value>>(),
            "maximized": self.maximized,
            "terminals": self
                .terminals
                .iter()
                .map(|(id, name)| (id.clone(), Value::from(name.as_str())))
                .collect::<Map<String, Value>>(),
            "docsSource": self.docs_source,
            "changesView": self.changes_view.name(),
            "filesWidth": self.files_width,
            "diff": self.diff.to_json(),
            "theme": self.theme,
            "lightTheme": self.themes.light,
            "darkTheme": self.themes.dark,
            "uiSize": self.ui_size,
            "zoomLevel": self.zoom_level,
            "terminalFont": self.terminal_font,
            "recent": self.recent,
            "whatsNewSeen": self.whats_new_seen,
            "cleanOnSave": self.clean_on_save,
        });
        format!("{value:#}\n")
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

/// Not a number: 0. A number out of range: the nearest level.
fn zoom_level_of(value: Option<&Value>) -> i32 {
    // `as` saturates, so a huge number still lands on a limit.
    clamp_zoom_level(value.and_then(Value::as_f64).unwrap_or(0.).round() as i32)
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
    folder_for(
        std::env::consts::OS,
        |name| std::env::var_os(name),
        Path::exists,
    )
}

/// `os` is `std::env::consts::OS`. Linux and the rest follow the XDG data folder.
pub fn folder_for(
    os: &str,
    var: impl Fn(&str) -> Option<std::ffi::OsString>,
    exists: impl Fn(&Path) -> bool,
) -> Option<PathBuf> {
    let set = |name: &str| {
        var(name)
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
    };
    match os {
        "macos" => Some(set("HOME")?.join("Library/Application Support/Natamux")),
        "windows" => natamux_client::config::windows_folder(&var, exists),
        _ => Some(
            set("XDG_CONFIG_HOME")
                .or_else(|| Some(set("HOME")?.join(".config")))?
                .join("Natamux"),
        ),
    }
}

#[path = "view_state_edits.rs"]
mod edits;

#[cfg(test)]
#[path = "view_state_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "view_state_background_tests.rs"]
mod background_tests;
