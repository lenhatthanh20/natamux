use super::*;
use crate::app::settings_rules::SettingRow;

fn repo(id: &str, name: &str) -> Repo {
    Repo {
        id: id.to_owned(),
        name: name.to_owned(),
        path: format!("/data/projects/{name}"),
        worktrees_dir: None,
    }
}

fn project(id: &str) -> NavItem {
    NavItem::Page(SettingsPage::Project(id.to_owned()))
}

#[test]
fn lists_the_app_pages_the_projects_then_add_project() {
    let items: Vec<NavItem> = nav_items(&[repo("r1", "natamux"), repo("r2", "shop")])
        .into_iter()
        .map(|(item, _)| item)
        .collect();
    assert_eq!(
        items,
        vec![
            NavItem::Page(SettingsPage::General),
            NavItem::Page(SettingsPage::Appearance),
            NavItem::Page(SettingsPage::Agents),
            NavItem::Shortcuts,
            project("r1"),
            project("r2"),
            NavItem::AddProject,
        ]
    );
    let labels: Vec<String> = nav_items(&[]).into_iter().map(|(_, label)| label).collect();
    assert_eq!(
        labels,
        [
            "General",
            "Appearance",
            "Agents",
            "Keyboard shortcuts",
            "Add project\u{2026}"
        ]
    );
}

#[test]
fn the_shown_page_is_selected_unless_a_search_shows() {
    let shown = SettingsPage::Agents;
    assert_eq!(
        nav_state(&NavItem::Page(SettingsPage::Agents), &shown, None),
        (true, false)
    );
    assert_eq!(
        nav_state(&NavItem::Page(SettingsPage::General), &shown, None),
        (false, false)
    );
    let found = SearchResults {
        rows: vec![SettingRow::CustomAgents],
        projects: Vec::new(),
    };
    assert_eq!(
        nav_state(&NavItem::Page(SettingsPage::Agents), &shown, Some(&found)),
        (false, false)
    );
}

#[test]
fn a_search_dims_rows_with_no_hit_and_always_the_shortcuts() {
    let shown = SettingsPage::General;
    let found = SearchResults {
        rows: vec![SettingRow::FontSize],
        projects: vec![("r1".to_owned(), "natamux".to_owned())],
    };
    let dim = |item: &NavItem| nav_state(item, &shown, Some(&found)).1;
    assert!(dim(&NavItem::Page(SettingsPage::General)));
    assert!(!dim(&NavItem::Page(SettingsPage::Appearance)));
    assert!(dim(&NavItem::Shortcuts));
    assert!(!dim(&project("r1")));
    assert!(dim(&project("r2")));
    assert!(!dim(&NavItem::AddProject));
    // Neither the shortcuts nor Add project is ever selected.
    assert_eq!(nav_state(&NavItem::Shortcuts, &shown, None), (false, false));
    assert_eq!(
        nav_state(&NavItem::AddProject, &shown, None),
        (false, false)
    );
}

#[test]
fn each_row_has_its_tile() {
    assert_eq!(
        nav_tile(&NavItem::Page(SettingsPage::General), "General"),
        NavTile::Icon("sliders-vertical", TileColor::Gray)
    );
    assert_eq!(
        nav_tile(&NavItem::Shortcuts, "Keyboard shortcuts"),
        NavTile::Icon("keyboard", TileColor::Yellow)
    );
    assert_eq!(
        nav_tile(&project("r1"), "natamux"),
        NavTile::Avatar("natamux".to_owned())
    );
    assert_eq!(nav_tile(&NavItem::AddProject, "Add"), NavTile::Dashed);
}
