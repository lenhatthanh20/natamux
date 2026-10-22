//! What the web keeps per browser (`natamux.*` in localStorage), kept in one file: `view.json`
//! in the app's data folder. The rules of each part are `app/view_rules.rs` and
//! `sidebar/sidebar_tree.rs`; this file reads and writes them.

use std::path::{Path, PathBuf};

use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value, json};

use crate::app::layout_mode::PANEL_WIDTH;
use crate::app::view_rules::{Entries, parse_pinned, with_seen};
use crate::sidebar::sidebar_tree::{Folds, with_fold};

pub const FILE_NAME: &str = "view.json";
/// `useStoredWidth('natamux.sidebar-width', PANEL_WIDTH, 220, 440)`.
pub const SIDEBAR_MIN: f32 = 220.0;
pub const SIDEBAR_MAX: f32 = 440.0;

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
mod tests {
    use super::*;

    fn temp_folder(name: &str) -> PathBuf {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("natamux-view-{name}-{unique}"))
    }

    #[test]
    fn a_saved_view_reads_back_the_same() {
        let mut state = ViewState {
            folds: vec![("repo:r1".into(), false), ("task:T-1".into(), true)],
            blurred: vec!["r2".into()],
            pinned: vec!["T-3".into(), "T-1".into()],
            layout: Layout::Board,
            sidebar_open: false,
            sidebar_width: 300.0,
            show_finished: true,
            seen: vec![
                ("T-1".into(), TaskState::Running),
                ("T-2".into(), TaskState::NeedsInput),
            ],
            last_base: Vec::new(),
        };
        state.remember_base("r1", "main");
        let folder = temp_folder("round");
        save(&folder, &state).unwrap();
        assert_eq!(load(&folder), state);
        assert!(!folder.join("view.json.partial").exists());
        std::fs::remove_dir_all(folder).ok();
    }

    #[test]
    fn a_missing_file_is_the_default_view() {
        assert_eq!(load(&temp_folder("missing")), ViewState::default());
    }

    #[test]
    fn a_broken_file_is_ignored_and_replaced_on_the_next_save() {
        let folder = temp_folder("broken");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join(FILE_NAME), "{\"pinned\": [\"T-1\"").unwrap();
        let mut state = load(&folder);
        assert_eq!(state, ViewState::default());
        state.pinned.push("T-2".into());
        save(&folder, &state).unwrap();
        assert_eq!(load(&folder).pinned, ["T-2"]);
        std::fs::remove_dir_all(folder).ok();
    }

    #[test]
    fn a_part_of_the_wrong_kind_falls_back_to_its_default() {
        let state = ViewState::parse(
            r#"{"folds": [1], "pinned": "T-1", "layout": "progress", "sidebarOpen": "no",
                "sidebarWidth": 9000, "seen": {"T-1": "running", "T-2": 4},
                "lastBase": {"r1": "dev", "r2": 3}}"#,
        );
        assert_eq!(state.folds, Vec::new());
        assert_eq!(state.pinned, Vec::<String>::new());
        assert_eq!(state.layout, Layout::Table);
        assert!(state.sidebar_open);
        assert_eq!(state.sidebar_width, SIDEBAR_MAX);
        assert_eq!(state.seen, vec![("T-1".into(), TaskState::Running)]);
        assert_eq!(state.last_base, vec![("r1".into(), "dev".into())]);
    }

    #[test]
    fn keeps_the_newest_300_folds_and_1000_seen_states() {
        let folds: Map<String, Value> = (0..310)
            .map(|at| (format!("task:T-{at}"), Value::Bool(true)))
            .collect();
        let seen: Map<String, Value> = (0..1010)
            .map(|at| (format!("T-{at}"), Value::from("done")))
            .collect();
        let text = json!({"folds": folds, "seen": seen}).to_string();
        let state = ViewState::parse(&text);
        assert_eq!(state.folds.len(), 300);
        assert_eq!(state.folds[0].0, "task:T-10");
        assert_eq!(state.seen.len(), 1000);
        assert_eq!(state.seen[0].0, "T-10");
    }

    #[test]
    fn remembers_one_base_per_project_and_blurs_by_toggle() {
        let mut state = ViewState::default();
        state.remember_base("r1", "main");
        state.remember_base("r2", "dev");
        state.remember_base("r1", "release");
        assert_eq!(state.last_base("r1"), Some("release"));
        assert_eq!(state.last_base("r3"), None);
        state.toggle_blurred("r1");
        state.toggle_blurred("r2");
        state.toggle_blurred("r1");
        assert_eq!(state.blurred, ["r2"]);
        state.set_sidebar_width(100.0);
        assert_eq!(state.sidebar_width, SIDEBAR_MIN);
    }

    #[test]
    fn the_data_folder_follows_each_os() {
        let env = |name: &str| match name {
            "HOME" => Some("/Users/an".into()),
            "APPDATA" => Some("C:\\Users\\an\\AppData\\Roaming".into()),
            _ => None,
        };
        assert_eq!(
            folder_for("macos", env),
            Some(PathBuf::from(
                "/Users/an/Library/Application Support/Natamux Native"
            ))
        );
        assert_eq!(
            folder_for("windows", env),
            Some(PathBuf::from("C:\\Users\\an\\AppData\\Roaming").join("Natamux Native"))
        );
        assert_eq!(
            folder_for("linux", env),
            Some(PathBuf::from("/Users/an/.local/share/natamux-native"))
        );
        let xdg = |name: &str| (name == "XDG_DATA_HOME").then(|| "/data".into());
        assert_eq!(
            folder_for("linux", xdg),
            Some(PathBuf::from("/data/natamux-native"))
        );
        assert_eq!(folder_for("macos", |_| None), None);
    }
}
