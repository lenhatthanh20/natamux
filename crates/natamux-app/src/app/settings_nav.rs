//! The rows of the Settings nav (layout-b 2): the app's pages, Keyboard shortcuts, one row per
//! project and "Add project…", with the tile, selected and dim state of each.

use natamux_proto::projects::repo::Repo;

use crate::app::settings_rules::{SearchResults, SettingsPage, page_tile};
use crate::components::colors::TileColor;

/// General, Appearance, Agents and Keyboard shortcuts come before the projects in the nav.
pub const APP_ITEMS: usize = 4;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavItem {
    Page(SettingsPage),
    /// Opens the shortcut sheet; not a page.
    Shortcuts,
    /// Opens the Projects dialog; always the last row.
    AddProject,
}

/// What a nav row shows before its label.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavTile {
    /// A Lucide icon name and its colour.
    Icon(&'static str, TileColor),
    /// A project's letter avatar, by its name.
    Avatar(String),
    /// No fill, a dashed outline with a plus.
    Dashed,
}

pub fn nav_items(repos: &[Repo]) -> Vec<(NavItem, String)> {
    let mut items = vec![
        (NavItem::Page(SettingsPage::General), "General".to_owned()),
        (
            NavItem::Page(SettingsPage::Appearance),
            "Appearance".to_owned(),
        ),
        (NavItem::Page(SettingsPage::Agents), "Agents".to_owned()),
        (NavItem::Shortcuts, "Keyboard shortcuts".to_owned()),
    ];
    items.extend(repos.iter().map(|repo| {
        let page = SettingsPage::Project(repo.id.clone());
        (NavItem::Page(page), repo.name.clone())
    }));
    items.push((NavItem::AddProject, "Add project\u{2026}".to_owned()));
    items
}

/// A nav row stays bright while a search finds something on its page.
fn nav_matches(item: &NavItem, found: &SearchResults) -> bool {
    match item {
        NavItem::Page(SettingsPage::Project(id)) => {
            found.projects.iter().any(|(project, _)| project == id)
        }
        NavItem::Page(page) => found.rows.iter().any(|row| &row.page() == page),
        NavItem::Shortcuts => false,
        NavItem::AddProject => true,
    }
}

/// (selected, dim) of a row. While a search shows its results no row is selected.
pub fn nav_state(
    item: &NavItem,
    shown: &SettingsPage,
    found: Option<&SearchResults>,
) -> (bool, bool) {
    let selected = found.is_none() && matches!(item, NavItem::Page(page) if page == shown);
    let dim = found.is_some_and(|found| !nav_matches(item, found));
    (selected, dim)
}

pub fn nav_tile(item: &NavItem, label: &str) -> NavTile {
    match item {
        NavItem::Page(SettingsPage::Project(_)) => NavTile::Avatar(label.to_owned()),
        NavItem::Page(page) => {
            page_tile(page).map_or(NavTile::Dashed, |(icon, color)| NavTile::Icon(icon, color))
        }
        NavItem::Shortcuts => NavTile::Icon("keyboard", TileColor::Yellow),
        NavItem::AddProject => NavTile::Dashed,
    }
}

#[cfg(test)]
#[path = "settings_nav_tests.rs"]
mod tests;
