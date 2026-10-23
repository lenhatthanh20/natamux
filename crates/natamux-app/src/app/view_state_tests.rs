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
        agent_panes: Some(vec!["T-2".into(), "T-1".into()]),
        pane_layout: PaneLayout::Two,
        details_width: 400.0,
        details_view: Some(DetailsView::Details),
        sections: vec![("notes".into(), false), ("details".into(), true)],
        notify: true,
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
fn the_details_panel_keeps_its_width_tab_and_folded_sections() {
    let state = ViewState::parse(
        r#"{"detailsWidth": 900, "detailsView": "progress", "sections": {"usage": false, "x": 1}}"#,
    );
    assert_eq!(state.details_width, DETAILS_MAX);
    assert_eq!(state.details_view, Some(DetailsView::Progress));
    assert!(!state.section_open("usage", true));
    assert!(state.section_open("x", true));
    assert!(!state.section_open("details", false));
    let broken = ViewState::parse(r#"{"detailsView": "docs", "notify": "yes"}"#);
    assert_eq!(broken.details_view, None);
    assert!(!broken.notify);
}

// The details cases of `view-store.test.ts`.

#[test]
fn starts_with_the_details_closed_when_nothing_was_stored() {
    let view = ViewState::parse("{}");
    assert_eq!(view.details_view, None);
    assert!(!crate::app::view_rules::details_open_for(None, false));
}

#[test]
fn does_not_store_the_details_flags() {
    let text = ViewState::default().to_json();
    assert!(!text.contains("detailOpen") && !text.contains("detailsOpen"));
}

#[test]
fn uses_the_view_the_user_picked_last() {
    use crate::app::view_rules::details_view_for;
    assert_eq!(
        details_view_for(Some(DetailsView::Details), true),
        DetailsView::Details
    );
    assert_eq!(
        details_view_for(Some(DetailsView::Progress), false),
        DetailsView::Progress
    );
}

#[test]
fn shows_the_progress_beside_an_open_workspace_and_the_details_over_the_list() {
    use crate::app::view_rules::details_view_for;
    assert_eq!(details_view_for(None, true), DetailsView::Progress);
    assert_eq!(details_view_for(None, false), DetailsView::Details);
}

#[test]
fn remembers_the_view_the_user_picks_also_on_the_next_visit() {
    let view = ViewState {
        details_view: Some(DetailsView::Details),
        ..ViewState::default()
    };
    assert_eq!(
        ViewState::parse(&view.to_json()).details_view,
        Some(DetailsView::Details)
    );
}

#[test]
fn starts_with_no_picked_view_when_nothing_or_something_unknown_was_stored() {
    assert_eq!(ViewState::parse("{}").details_view, None);
    assert_eq!(
        ViewState::parse(r#"{"detailsView": "docs"}"#).details_view,
        None
    );
    assert_eq!(ViewState::parse(r#"{"detailsView": 3}"#).details_view, None);
}

#[test]
fn stores_progress_again_when_the_user_picks_it_again() {
    let mut view = ViewState {
        details_view: Some(DetailsView::Details),
        ..ViewState::default()
    };
    view.details_view = Some(DetailsView::Progress);
    assert_eq!(
        ViewState::parse(&view.to_json()).details_view,
        Some(DetailsView::Progress)
    );
}

#[test]
fn keeps_the_newest_folded_sections() {
    let mut state = ViewState::default();
    for at in 0..60 {
        state.set_section(&format!("s{at}"), false);
    }
    state.set_section("s10", true);
    assert_eq!(state.sections.len(), MAX_SECTIONS);
    assert_eq!(state.sections.last(), Some(&("s10".to_string(), true)));
    assert!(state.section_open("s0", true));
}

#[test]
fn the_agents_grid_keeps_the_users_list_and_columns() {
    let chosen = ViewState::parse(r#"{"agentPanes": ["T-2", 3, "T-1"], "paneLayout": "4"}"#);
    assert_eq!(chosen.agent_panes, Some(vec!["T-2".into(), "T-1".into()]));
    assert_eq!(chosen.pane_layout, PaneLayout::Four);
    let default = ViewState::parse(r#"{"agentPanes": null, "paneLayout": "9"}"#);
    assert_eq!(default.agent_panes, None);
    assert_eq!(default.pane_layout, PaneLayout::Auto);
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

#[test]
fn opens_the_details_of_a_task_on_its_progress() {
    let mut view = ViewState {
        details_view: Some(DetailsView::Details),
        ..ViewState::default()
    };
    let open = view.show_progress(&vec![("T-1".into(), false)], "T-2");
    assert_eq!(view.details_view, Some(DetailsView::Progress));
    assert_eq!(
        open,
        vec![("T-1".to_string(), false), ("T-2".to_string(), true)]
    );
}
