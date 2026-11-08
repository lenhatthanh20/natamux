use super::*;
use crate::components::colors::TileColor;

fn repo(id: &str, name: &str) -> Repo {
    Repo {
        id: id.to_owned(),
        name: name.to_owned(),
        path: format!("/data/projects/{name}"),
        worktrees_dir: None,
    }
}

#[test]
fn opens_on_the_asked_page_else_the_shown_one_else_the_last_one() {
    let last = SettingsPage::General;
    let shown = SettingsPage::Project("r1".to_owned());
    assert_eq!(
        page_to_open(Some(SettingsPage::Appearance), Some(&shown), &last),
        SettingsPage::Appearance
    );
    assert_eq!(page_to_open(None, Some(&shown), &last), shown);
    assert_eq!(page_to_open(None, None, &last), SettingsPage::General);
}

#[test]
fn a_project_that_is_gone_shows_appearance() {
    let repos = [repo("r1", "natamux")];
    let kept = SettingsPage::Project("r1".to_owned());
    assert_eq!(shown_page(&kept, &repos), kept);
    let gone = SettingsPage::Project("r2".to_owned());
    assert_eq!(shown_page(&gone, &repos), SettingsPage::Appearance);
    assert_eq!(
        shown_page(&SettingsPage::General, &[]),
        SettingsPage::General
    );
}

#[test]
fn a_project_page_is_titled_with_the_project_name() {
    let repos = [repo("r1", "natamux")];
    assert_eq!(
        page_title(&SettingsPage::Project("r1".to_owned()), &repos),
        "natamux"
    );
    assert_eq!(page_title(&SettingsPage::Appearance, &repos), "Appearance");
    assert_eq!(
        page_title(&SettingsPage::Project("r9".to_owned()), &repos),
        "r9"
    );
}

#[test]
fn rows_sit_on_their_pages() {
    for row in [
        SettingRow::Notify,
        SettingRow::NotifyAlways,
        SettingRow::UpdatesVersion,
        SettingRow::UpdatesAuto,
        SettingRow::CleanOnSave,
    ] {
        assert_eq!(row.page(), SettingsPage::General);
    }
    for row in [
        SettingRow::Theme,
        SettingRow::LightTheme,
        SettingRow::DarkTheme,
        SettingRow::UiSize,
        SettingRow::SidebarIds,
        SettingRow::FontFamily,
        SettingRow::FontSize,
    ] {
        assert_eq!(row.page(), SettingsPage::Appearance);
    }
}

#[test]
fn the_agents_page_is_found_by_its_words() {
    assert_eq!(SettingRow::CustomAgents.page(), SettingsPage::Agents);
    assert_eq!(page_title(&SettingsPage::Agents, &[]), "Agents");
    for query in ["agent", "custom agent", "command", "model"] {
        let found = search(query, &[]).unwrap_or_default();
        assert!(found.rows.contains(&SettingRow::CustomAgents), "{query}");
    }
}

#[test]
fn an_empty_query_shows_the_page() {
    assert_eq!(search("", &[]), None);
    assert_eq!(search("   ", &[]), None);
}

#[test]
fn finds_the_font_rows_by_their_words() {
    let repos = [repo("r1", "natamux")];
    let found = search("font", &repos);
    assert_eq!(
        found,
        Some(SearchResults {
            rows: vec![SettingRow::FontFamily, SettingRow::FontSize],
            projects: Vec::new(),
        })
    );
}

#[test]
fn finds_a_row_by_a_keyword() {
    let found = search("dark", &[]);
    assert_eq!(
        found.map(|found| found.rows),
        Some(vec![SettingRow::Theme, SettingRow::DarkTheme])
    );
}

#[test]
fn finds_the_task_ids_switch_by_id_key_or_sidebar() {
    for query in ["id", "key", "sidebar"] {
        let found = search(query, &[]).unwrap_or_default();
        assert!(found.rows.contains(&SettingRow::SidebarIds), "{query}");
    }
}

#[test]
fn finds_the_theme_rows_by_a_theme_name_or_color_scheme() {
    let rows = |query: &str| search(query, &[]).map(|found| found.rows);
    assert_eq!(rows("dracula"), Some(vec![SettingRow::DarkTheme]));
    assert_eq!(rows("latte"), Some(vec![SettingRow::LightTheme]));
    assert_eq!(
        rows("color scheme"),
        Some(vec![SettingRow::LightTheme, SettingRow::DarkTheme])
    );
    let theme = rows("theme").unwrap_or_default();
    for row in [
        SettingRow::Theme,
        SettingRow::LightTheme,
        SettingRow::DarkTheme,
    ] {
        assert!(theme.contains(&row), "{row:?}");
    }
}

#[test]
fn finds_the_update_rows() {
    let found = search("update", &[]);
    assert_eq!(
        found.map(|found| found.rows),
        Some(vec![SettingRow::UpdatesVersion, SettingRow::UpdatesAuto])
    );
}

#[test]
fn finds_a_project_by_its_name() {
    let repos = [repo("r1", "natamux"), repo("r2", "storefront")];
    let found = search("storefront", &repos);
    assert_eq!(
        found,
        Some(SearchResults {
            rows: Vec::new(),
            projects: vec![("r2".to_owned(), "storefront".to_owned())],
        })
    );
}

#[test]
fn finds_the_projects_for_the_send_back_limit() {
    for query in ["send back", "limit"] {
        let found = search(query, &[repo("r1", "natamux")]);
        assert_eq!(
            found.map(|found| found.projects),
            Some(vec![("r1".to_owned(), "natamux".to_owned())]),
            "{query}"
        );
    }
}

#[test]
fn a_query_that_matches_nothing_finds_nothing() {
    let found = search("qqzz", &[repo("r1", "natamux")]);
    assert!(found.is_some_and(|found| found.is_empty()));
}

#[test]
fn steps_the_font_size_from_the_field_else_the_saved_size() {
    assert_eq!(font_size_step("", None, 1.), BUILT_IN_SIZE + 1.);
    assert_eq!(font_size_step("20", Some(13.), -1.), 19.);
    // A size out of range in the field steps from the saved one.
    assert_eq!(font_size_step("99", Some(15.), 1.), 16.);
    assert_eq!(font_size_step("", Some(15.), 1.), 16.);
}

#[test]
fn keeps_the_font_size_from_8_to_32() {
    assert_eq!(font_size_step("32", None, 1.), MAX_SIZE);
    assert_eq!(font_size_step("8", None, -1.), MIN_SIZE);
}

#[test]
fn finds_the_version_row_by_check() {
    let found = search("check now", &[]).unwrap_or_default();
    assert!(found.rows.contains(&SettingRow::UpdatesVersion));
}

#[test]
fn app_pages_draw_their_rows_in_groups() {
    assert_eq!(
        page_groups(&SettingsPage::General),
        vec![
            (
                "Notifications",
                vec![SettingRow::Notify, SettingRow::NotifyAlways]
            ),
            (
                "Updates",
                vec![SettingRow::UpdatesVersion, SettingRow::UpdatesAuto]
            ),
            ("Editor", vec![SettingRow::CleanOnSave]),
        ]
    );
    assert_eq!(
        page_groups(&SettingsPage::Appearance),
        vec![
            (
                "Theme",
                vec![
                    SettingRow::Theme,
                    SettingRow::LightTheme,
                    SettingRow::DarkTheme,
                    SettingRow::UiSize,
                ]
            ),
            ("Sidebar", vec![SettingRow::SidebarIds]),
            (
                "Terminal font",
                vec![SettingRow::FontFamily, SettingRow::FontSize]
            ),
        ]
    );
    // The Custom agents row shows in the results only; the Agents page draws its own lists.
    assert!(page_groups(&SettingsPage::Agents).is_empty());
    assert!(page_groups(&SettingsPage::Project("r1".to_owned())).is_empty());
}

#[test]
fn a_group_lists_rows_of_its_own_page_only() {
    for page in [SettingsPage::General, SettingsPage::Appearance] {
        for (group, rows) in page_groups(&page) {
            for row in rows {
                assert_eq!(row.page(), page, "{row:?}");
                assert_eq!(row.group(), group, "{row:?}");
            }
        }
    }
    assert_eq!(SettingRow::CustomAgents.group(), "Custom agents");
}

#[test]
fn app_pages_have_a_tile_and_a_summary() {
    assert_eq!(
        page_tile(&SettingsPage::General),
        Some(("sliders-vertical", TileColor::Gray))
    );
    assert_eq!(
        page_tile(&SettingsPage::Appearance),
        Some(("palette", TileColor::Blue))
    );
    assert_eq!(
        page_tile(&SettingsPage::Agents),
        Some(("bot", TileColor::Green))
    );
    assert_eq!(page_tile(&SettingsPage::Project("r1".to_owned())), None);
    assert_eq!(
        page_summary(&SettingsPage::General),
        "Notifications, updates and the editor \u{b7} changes apply at once"
    );
    assert_eq!(
        page_summary(&SettingsPage::Appearance),
        "Theme, UI size, sidebar and terminal font \u{b7} changes apply at once"
    );
    assert_eq!(
        page_summary(&SettingsPage::Agents),
        "Built-in agents and your own agent commands \u{b7} custom agents need Save"
    );
}

#[test]
fn results_group_rows_by_page_in_page_order() {
    let found = SearchResults {
        rows: vec![
            SettingRow::FontSize,
            SettingRow::CustomAgents,
            SettingRow::Notify,
            SettingRow::Theme,
        ],
        projects: Vec::new(),
    };
    assert_eq!(
        results_groups(&found),
        vec![
            (SettingsPage::General, "General", vec![SettingRow::Notify]),
            (
                SettingsPage::Appearance,
                "Appearance",
                vec![SettingRow::FontSize, SettingRow::Theme]
            ),
            (
                SettingsPage::Agents,
                "Agents",
                vec![SettingRow::CustomAgents]
            ),
        ]
    );
    assert!(results_groups(&SearchResults::default()).is_empty());
}

#[test]
fn the_results_summary_counts_rows_and_projects() {
    let found = SearchResults {
        rows: vec![SettingRow::FontFamily, SettingRow::FontSize],
        projects: vec![("r1".to_owned(), "natamux".to_owned())],
    };
    assert_eq!(
        results_summary(&found, " font "),
        "3 matches for \u{201c}font\u{201d} \u{b7} the controls work here"
    );
    let one = SearchResults {
        rows: vec![SettingRow::Notify],
        projects: Vec::new(),
    };
    assert_eq!(
        results_summary(&one, "notify"),
        "1 match for \u{201c}notify\u{201d} \u{b7} the controls work here"
    );
    assert_eq!(
        no_match_text(" qqzz "),
        "No settings match \u{201c}qqzz\u{201d}"
    );
}

#[test]
fn the_popup_is_centered_and_keeps_a_margin_on_small_windows() {
    let at = |w, h| {
        let b = popup_bounds(w, h);
        (b.left, b.top, b.width, b.height)
    };
    assert_eq!(at(1440., 900.), (200., 90., 1040., 720.));
    assert_eq!(at(1024., 768.), (32., 48., 960., 672.));
    assert_eq!(at(40., 50.), (20., 25., 0., 0.));
}
