//! What the web keeps per browser (`natamux.*` in localStorage), kept in one file: `view.json`
//! in the app's data folder. The rules of each part are `app/view_rules.rs` and
//! `sidebar/sidebar_tree.rs`; this file reads and writes them.

use std::path::{Path, PathBuf};

use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value, json};

use crate::app::layout_mode::PANEL_WIDTH;
use crate::app::view_rules::{DetailsView, Entries, parse_pinned, with_seen};
use crate::sidebar::sidebar_tree::{Folds, with_fold};
use crate::terminals::pane_rules::{PaneLayout, parse_pane_layout};

pub const FILE_NAME: &str = "view.json";
/// `useStoredWidth('natamux.sidebar-width', PANEL_WIDTH, 220, 440)`.
pub const SIDEBAR_MIN: f32 = 220.0;
pub const SIDEBAR_MAX: f32 = 440.0;
/// `useStoredWidth('natamux.details-width', PANEL_WIDTH, 320, 560)`.
pub const DETAILS_MIN: f32 = 320.0;
pub const DETAILS_MAX: f32 = 560.0;
/// Folded sections of the details panel kept at most (`natamux.section.<id>`).
const MAX_SECTIONS: usize = 50;

/// The progress layout is not in the app yet, so a saved progress opens the table.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layout {
    #[default]
    Table,
    Board,
}

impl Layout {
    pub fn name(self) -> &'static str {
        match self {
            Layout::Table => "table",
            Layout::Board => "board",
        }
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
    /// System notifications for tasks that start to need the user (off until switched on).
    pub notify: bool,
}

impl Default for ViewState {
    fn default() -> Self {
        ViewState {
            folds: Vec::new(),
            blurred: Vec::new(),
            pinned: Vec::new(),
            layout: Layout::Table,
            sidebar_open: true,
            sidebar_width: PANEL_WIDTH,
            show_finished: false,
            seen: Vec::new(),
            last_base: Vec::new(),
            agent_panes: None,
            pane_layout: PaneLayout::default(),
            details_width: PANEL_WIDTH,
            details_view: None,
            sections: Vec::new(),
            notify: false,
        }
    }
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
        // Through the rules, so the caps hold for a file written by hand too.
        let folds = pairs("folds")
            .into_iter()
            .filter_map(|(key, open)| open.as_bool().map(|open| (key, open)))
            .fold(Vec::new(), |folds, (key, open)| {
                with_fold(&folds, &key, open)
            });
        let seen_pairs: Vec<(String, TaskState)> = pairs("seen")
            .into_iter()
            .filter_map(|(id, state)| Some((id, serde_json::from_value(state).ok()?)))
            .collect();
        let seen_refs: Vec<(&str, TaskState)> = seen_pairs
            .iter()
            .map(|(id, state)| (id.as_str(), *state))
            .collect();
        ViewState {
            folds,
            blurred: strings(map.get("blurred")),
            pinned: strings(map.get("pinned")),
            layout: match map.get("layout").and_then(Value::as_str) {
                Some("board") => Layout::Board,
                _ => Layout::Table,
            },
            sidebar_open: flag(map.get("sidebarOpen"), default.sidebar_open),
            sidebar_width: map
                .get("sidebarWidth")
                .and_then(Value::as_f64)
                .map_or(default.sidebar_width, |width| {
                    (width as f32).clamp(SIDEBAR_MIN, SIDEBAR_MAX)
                }),
            show_finished: flag(map.get("showFinished"), default.show_finished),
            seen: with_seen(&[], &seen_refs, true),
            last_base: pairs("lastBase")
                .into_iter()
                .filter_map(|(repo, base)| Some((repo, base.as_str()?.to_owned())))
                .collect(),
            agent_panes: match map.get("agentPanes") {
                Some(Value::Array(_)) => Some(strings(map.get("agentPanes"))),
                _ => None,
            },
            pane_layout: parse_pane_layout(map.get("paneLayout").and_then(Value::as_str)),
            details_width: map
                .get("detailsWidth")
                .and_then(Value::as_f64)
                .map_or(default.details_width, |width| {
                    (width as f32).clamp(DETAILS_MIN, DETAILS_MAX)
                }),
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
            notify: flag(map.get("notify"), default.notify),
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
            "notify": self.notify,
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

    /// `showProgress`: the details of `task_id` open on Progress. Gives the new open flags.
    pub fn show_progress(&mut self, open: &Entries<bool>, task_id: &str) -> Entries<bool> {
        self.details_view = Some(DetailsView::Progress);
        crate::app::view_rules::with_detail_open(open, task_id, true)
    }

    pub fn set_details_width(&mut self, width: f32) {
        self.details_width = width.clamp(DETAILS_MIN, DETAILS_MAX);
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
        "macos" => Some(set("HOME")?.join("Library/Application Support/Natamux Native")),
        "windows" => Some(set("APPDATA")?.join("Natamux Native")),
        _ => Some(
            set("XDG_DATA_HOME")
                .or_else(|| Some(set("HOME")?.join(".local/share")))?
                .join("natamux-native"),
        ),
    }
}

#[cfg(test)]
#[path = "view_state_tests.rs"]
mod tests;
