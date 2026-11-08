//! The rules of the Settings popup (T-0174, T-0306): its box, its pages, the page it opens on
//! and shows, the search over the app's rows and the projects, and the steps of the font size.

use natamux_proto::projects::repo::Repo;

use crate::components::colors::TileColor;
use crate::palette::fuzzy::fuzzy_match;
use crate::terminals::terminal_font::{BUILT_IN_SIZE, MAX_SIZE, MIN_SIZE, size_ok};

/// The Agents page header's summary line.
pub const AGENTS_SUMMARY: &str =
    "Built-in agents and your own agent commands · custom agents need Save";

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

/// The Settings popup's box in the window, in px (T-0306, mockup A). Fixed px, not rems, so it
/// fits the window at every UI size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PopupBounds {
    pub left: f32,
    pub top: f32,
    pub width: f32,
    pub height: f32,
}

/// At most 1040 x 720, at least 32 px from the window's sides and 48 px from its top and bottom,
/// centered both ways.
pub fn popup_bounds(window_width: f32, window_height: f32) -> PopupBounds {
    let width = 1040_f32.min(window_width - 64.).max(0.);
    let height = 720_f32.min(window_height - 96.).max(0.);
    PopupBounds {
        left: (window_width - width) / 2.,
        top: (window_height - height) / 2.,
        width,
        height,
    }
}

/// The size in px of a sheet that fills the Settings popup (T-0313, the stage sheet): 24 px from
/// each side of the popup, at most 960 x 640. It is centered in the window, as the popup is.
pub fn sheet_fill_size(window_width: f32, window_height: f32) -> (f32, f32) {
    let popup = popup_bounds(window_width, window_height);
    (
        960_f32.min(popup.width - 48.).max(0.),
        640_f32.min(popup.height - 48.).max(0.),
    )
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

/// The tile of an app page in the nav and its header: (Lucide icon name, colour). A project
/// shows its letter avatar instead.
pub fn page_tile(page: &SettingsPage) -> Option<(&'static str, TileColor)> {
    match page {
        SettingsPage::General => Some(("sliders-vertical", TileColor::Gray)),
        SettingsPage::Appearance => Some(("palette", TileColor::Blue)),
        SettingsPage::Agents => Some(("bot", TileColor::Green)),
        SettingsPage::Project(_) => None,
    }
}

/// The line under an app page's title. A project page writes its own.
pub fn page_summary(page: &SettingsPage) -> &'static str {
    match page {
        SettingsPage::General => "Notifications and updates \u{b7} changes apply at once",
        SettingsPage::Appearance => {
            "Theme, UI size, sidebar and terminal font \u{b7} changes apply at once"
        }
        SettingsPage::Agents => AGENTS_SUMMARY,
        SettingsPage::Project(_) => "",
    }
}

/// The rows of the app's pages, in the order they are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SettingRow {
    Notify,
    NotifyAlways,
    /// The version, the updater's state and Check now (or Restart to Update).
    UpdatesVersion,
    UpdatesAuto,
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
        SettingRow::NotifyAlways,
        SettingRow::UpdatesVersion,
        SettingRow::UpdatesAuto,
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
            | SettingRow::NotifyAlways
            | SettingRow::UpdatesVersion
            | SettingRow::UpdatesAuto => SettingsPage::General,
            SettingRow::CustomAgents => SettingsPage::Agents,
            _ => SettingsPage::Appearance,
        }
    }

    /// The group title the row sits under on its page.
    pub fn group(self) -> &'static str {
        match self {
            SettingRow::Notify | SettingRow::NotifyAlways => "Notifications",
            SettingRow::UpdatesVersion | SettingRow::UpdatesAuto => "Updates",
            SettingRow::Theme
            | SettingRow::LightTheme
            | SettingRow::DarkTheme
            | SettingRow::UiSize => "Theme",
            SettingRow::SidebarIds => "Sidebar",
            SettingRow::FontFamily | SettingRow::FontSize => "Terminal font",
            SettingRow::CustomAgents => "Custom agents",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SettingRow::Notify => "Notify me",
            SettingRow::NotifyAlways => "Always notify",
            SettingRow::UpdatesVersion => "Version",
            SettingRow::UpdatesAuto => "Check for updates automatically",
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
            SettingRow::NotifyAlways => "With Notify me on, also notify while you use the app.",
            SettingRow::UpdatesVersion => "The running version and the last update check.",
            SettingRow::UpdatesAuto => {
                "Look up new versions 30 seconds after start, then every 6 hours."
            }
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
            SettingRow::NotifyAlways => "notifications alert focused active",
            SettingRow::UpdatesVersion => "update version release check now",
            SettingRow::UpdatesAuto => "update version release automatically",
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

/// The groups of an app page: (title, rows). The Agents page draws its own lists, so its one
/// row shows in the results only.
pub fn page_groups(page: &SettingsPage) -> Vec<(&'static str, Vec<SettingRow>)> {
    if !matches!(page, SettingsPage::General | SettingsPage::Appearance) {
        return Vec::new();
    }
    let mut groups: Vec<(&'static str, Vec<SettingRow>)> = Vec::new();
    for row in SettingRow::ALL
        .into_iter()
        .filter(|row| &row.page() == page)
    {
        match groups.last_mut() {
            Some((title, rows)) if *title == row.group() => rows.push(row),
            _ => groups.push((row.group(), vec![row])),
        }
    }
    groups
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

/// The rows found, by page in nav order: (page, its title, rows in the order found).
pub fn results_groups(found: &SearchResults) -> Vec<(SettingsPage, &'static str, Vec<SettingRow>)> {
    [
        (SettingsPage::General, "General"),
        (SettingsPage::Appearance, "Appearance"),
        (SettingsPage::Agents, "Agents"),
    ]
    .into_iter()
    .filter_map(|(page, title)| {
        let rows: Vec<SettingRow> = found
            .rows
            .iter()
            .copied()
            .filter(|row| row.page() == page)
            .collect();
        (!rows.is_empty()).then_some((page, title, rows))
    })
    .collect()
}

/// The line under the Results title.
pub fn results_summary(found: &SearchResults, query: &str) -> String {
    let count = found.rows.len() + found.projects.len();
    let noun = if count == 1 { "match" } else { "matches" };
    format!(
        "{count} {noun} for \u{201c}{}\u{201d} \u{b7} the controls work here",
        query.trim()
    )
}

/// The Results page when nothing matches.
pub fn no_match_text(query: &str) -> String {
    format!("No settings match \u{201c}{}\u{201d}", query.trim())
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
        .filter(|row| hit(&[row.label(), row.description(), row.keywords(), row.group()]))
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
