//! The rules of the Settings page (T-0174): its pages, the page it opens on and shows, the search
//! over the app's rows and the projects, and the steps of the font size.

use natamux_proto::projects::repo::Repo;

use crate::palette::fuzzy::fuzzy_match;
use crate::terminals::terminal_font::{BUILT_IN_SIZE, MAX_SIZE, MIN_SIZE, size_ok};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SettingsPage {
    General,
    Appearance,
    /// The built-in agents and the custom ones (`custom-agents.json`).
    Agents,
    /// The settings of the project with this id (`.natamux.json`).
    Project(String),
}

/// Where focus goes when Settings opens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingsFocus {
    Search,
    /// The Font family field (the account menu's "Terminal font…").
    Font,
    /// The page itself (a project's Settings button).
    Page,
}

/// The page Settings opens on: the one asked for, else the one on screen, else the last one.
pub fn page_to_open(
    asked: Option<SettingsPage>,
    shown: Option<&SettingsPage>,
    last: &SettingsPage,
) -> SettingsPage {
    asked
        .or_else(|| shown.cloned())
        .unwrap_or_else(|| last.clone())
}

/// The page drawn: a project that is gone shows Appearance.
pub fn shown_page(page: &SettingsPage, repos: &[Repo]) -> SettingsPage {
    match page {
        SettingsPage::Project(id) if !repos.iter().any(|repo| &repo.id == id) => {
            SettingsPage::Appearance
        }
        page => page.clone(),
    }
}

/// The title of a page: its name, or the project's.
pub fn page_title(page: &SettingsPage, repos: &[Repo]) -> String {
    match page {
        SettingsPage::General => "General".to_owned(),
        SettingsPage::Appearance => "Appearance".to_owned(),
        SettingsPage::Agents => "Agents".to_owned(),
        SettingsPage::Project(id) => repos
            .iter()
            .find(|repo| &repo.id == id)
            .map_or_else(|| id.clone(), |repo| repo.name.clone()),
    }
}

/// The rows of the app's pages, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingRow {
    Notify,
    UpdatesVersion,
    UpdatesAuto,
    UpdatesCheck,
    Theme,
    LightTheme,
    DarkTheme,
    UiSize,
    SidebarIds,
    FontFamily,
    FontSize,
    CustomAgents,
}

impl SettingRow {
    pub const ALL: [SettingRow; 12] = [
        SettingRow::Notify,
        SettingRow::UpdatesVersion,
        SettingRow::UpdatesAuto,
        SettingRow::UpdatesCheck,
        SettingRow::Theme,
        SettingRow::LightTheme,
        SettingRow::DarkTheme,
        SettingRow::UiSize,
        SettingRow::SidebarIds,
        SettingRow::FontFamily,
        SettingRow::FontSize,
        SettingRow::CustomAgents,
    ];

    pub fn page(self) -> SettingsPage {
        match self {
            SettingRow::Notify
            | SettingRow::UpdatesVersion
            | SettingRow::UpdatesAuto
            | SettingRow::UpdatesCheck => SettingsPage::General,
            SettingRow::CustomAgents => SettingsPage::Agents,
            _ => SettingsPage::Appearance,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SettingRow::Notify => "Notify me",
            SettingRow::UpdatesVersion => "Version",
            SettingRow::UpdatesAuto => "Check for updates automatically",
            SettingRow::UpdatesCheck => "Check for updates",
            SettingRow::Theme => "Theme",
            SettingRow::LightTheme => "Light theme",
            SettingRow::DarkTheme => "Dark theme",
            SettingRow::UiSize => "UI size",
            SettingRow::SidebarIds => "Task ids in the sidebar",
            SettingRow::FontFamily => "Font family",
            SettingRow::FontSize => "Font size",
            SettingRow::CustomAgents => "Custom agents",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            SettingRow::Notify => "Show a system notification when a task starts to need you.",
            SettingRow::UpdatesVersion => "The running version and the last update check.",
            SettingRow::UpdatesAuto => {
                "Look up new versions 30 seconds after start, then every 6 hours."
            }
            SettingRow::UpdatesCheck => "Look up a new version now.",
            SettingRow::Theme => "Follow the system, or always light or dark.",
            SettingRow::LightTheme => "The theme used when the app is light.",
            SettingRow::DarkTheme => "The theme used when the app is dark.",
            SettingRow::UiSize => "Text and row size of the whole app.",
            SettingRow::SidebarIds => "Show each task's id ahead of its title in the sidebar.",
            SettingRow::FontFamily => "The font of the terminals. Empty uses the built-in Lilex.",
            SettingRow::FontSize => "The size of the terminal font, from 8 to 32.",
            SettingRow::CustomAgents => "Run an agent with your own command.",
        }
    }

    /// Other words a person may search for.
    fn keywords(self) -> &'static str {
        match self {
            SettingRow::Notify => "notifications alert",
            SettingRow::UpdatesVersion | SettingRow::UpdatesAuto | SettingRow::UpdatesCheck => {
                "update version release"
            }
            SettingRow::Theme => "dark light system colors",
            SettingRow::LightTheme => {
                "color scheme colors ayu catppuccin latte github gruvbox rose pine dawn solarized"
            }
            SettingRow::DarkTheme => {
                "color scheme colors ayu catppuccin mocha dracula github gruvbox nord rose pine \
                 solarized tokyo night"
            }
            SettingRow::UiSize => "compact large zoom",
            SettingRow::SidebarIds => "task id key",
            SettingRow::FontFamily | SettingRow::FontSize => "terminal font",
            SettingRow::CustomAgents => "agent cli program model",
        }
    }
}

/// What a search finds: rows of the app's pages, then projects as (id, name).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchResults {
    pub rows: Vec<SettingRow>,
    pub projects: Vec<(String, String)>,
}

impl SearchResults {
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty() && self.projects.is_empty()
    }
}

const PROJECT_KEYWORDS: &str = "project stages issues json pipeline send back limit";

/// None for an empty query: the page shows, not the results. Rows keep their page order.
pub fn search(query: &str, repos: &[Repo]) -> Option<SearchResults> {
    let query = query.trim();
    if query.is_empty() {
        return None;
    }
    let hit = |texts: &[&str]| texts.iter().any(|text| fuzzy_match(query, text).is_some());
    let rows = SettingRow::ALL
        .into_iter()
        .filter(|row| hit(&[row.label(), row.description(), row.keywords()]))
        .collect();
    let projects = repos
        .iter()
        .filter(|repo| hit(&[&repo.name, PROJECT_KEYWORDS]))
        .map(|repo| (repo.id.clone(), repo.name.clone()))
        .collect();
    Some(SearchResults { rows, projects })
}

/// The size after − or +: from the field when it holds a valid size, else from the saved size,
/// kept from 8 to 32.
pub fn font_size_step(field: &str, saved: Option<f32>, delta: f32) -> f32 {
    let from = size_ok(field, saved.unwrap_or(BUILT_IN_SIZE))
        .or(saved)
        .unwrap_or(BUILT_IN_SIZE);
    (from + delta).clamp(MIN_SIZE, MAX_SIZE)
}

#[cfg(test)]
#[path = "settings_rules_tests.rs"]
mod tests;
