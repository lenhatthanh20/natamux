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
        tabs: vec![
            ("T-1".into(), WorkspaceTab::Changes),
            ("T-2".into(), WorkspaceTab::History),
        ],
        docs_source: true,
        changes_view: FileViewMode::Tree,
        files_width: 300,
        diff: DiffFlags {
            side_by_side: false,
            collapse_unchanged: false,
            ignore_whitespace: false,
            show_moves: true,
            wrap_prose: false,
            wrap_code: true,
        },
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
        r#"{"folds": [1], "pinned": "T-1", "layout": "grid", "sidebarOpen": "no",
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

#[test]
fn the_changes_tab_starts_with_the_defaults_of_the_web() {
    let state = ViewState::parse("{}");
    assert!(state.tabs.is_empty());
    assert_eq!(state.changes_view, FileViewMode::List);
    assert_eq!(state.files_width, 260);
    assert_eq!(
        state.diff,
        DiffFlags {
            side_by_side: true,
            collapse_unchanged: true,
            ignore_whitespace: true,
            show_moves: false,
            wrap_prose: true,
            wrap_code: false,
        }
    );
    assert_eq!(state, ViewState::default());
}

#[test]
fn reads_the_tabs_the_view_and_the_flags_of_the_changes_tab() {
    let state = ViewState::parse(
        r#"{"tabs": {"T-1": "terminal", "T-2": "changes", "T-3": "history"},
            "changesView": "tree", "filesWidth": 330,
            "diff": {"sideBySide": false, "showMoves": true}}"#,
    );
    assert_eq!(state.tab_of("T-1"), Some(WorkspaceTab::Terminal));
    assert_eq!(state.tab_of("T-2"), Some(WorkspaceTab::Changes));
    assert_eq!(state.tab_of("T-3"), Some(WorkspaceTab::History));
    assert_eq!(state.tab_of("T-4"), None);
    assert_eq!(state.changes_view, FileViewMode::Tree);
    assert_eq!(state.files_width, 330);
    assert!(!state.diff.side_by_side && state.diff.show_moves);
    assert!(state.diff.collapse_unchanged && state.diff.wrap_prose);
}

#[test]
fn a_bad_value_of_the_changes_tab_is_its_default() {
    let state = ViewState::parse(
        r#"{"tabs": {"T-1": "docs", "T-2": "editor", "T-3": "nope", "T-4": 3, "T-5": "changes"},
            "changesView": "grid", "filesWidth": "wide", "diff": {"wrapCode": "yes", "showMoves": 1}}"#,
    );
    assert_eq!(
        state.tabs,
        vec![
            ("T-1".into(), WorkspaceTab::Docs),
            ("T-5".into(), WorkspaceTab::Changes)
        ]
    );
    assert_eq!(state.changes_view, FileViewMode::List);
    assert_eq!(state.files_width, FILES_DEFAULT);
    assert_eq!(state.diff, DiffFlags::default());
    let all = ViewState::parse(r#"{"tabs": [1], "changesView": 4, "diff": "x"}"#);
    assert_eq!(all, ViewState::default());
}

#[test]
fn the_files_width_stays_between_200_and_420() {
    assert_eq!(ViewState::parse(r#"{"filesWidth": 50}"#).files_width, 200);
    assert_eq!(ViewState::parse(r#"{"filesWidth": 9000}"#).files_width, 420);
    assert_eq!(ViewState::parse(r#"{"filesWidth": -5}"#).files_width, 200);
    let mut state = ViewState::default();
    state.set_files_width(100);
    assert_eq!(state.files_width, 200);
    state.set_files_width(500);
    assert_eq!(state.files_width, 420);
}

#[test]
fn keeps_the_newest_200_tabs() {
    let tabs: Map<String, Value> = (0..205)
        .map(|at| (format!("T-{at}"), Value::from("changes")))
        .collect();
    let state = ViewState::parse(&json!({ "tabs": tabs }).to_string());
    assert_eq!(state.tabs.len(), 200);
    assert_eq!(state.tabs[0].0, "T-5");
    assert_eq!(state.tab_of("T-204"), Some(WorkspaceTab::Changes));
}

#[test]
fn remembers_the_last_tab_of_each_workspace_in_the_file() {
    let mut state = ViewState::default();
    state.remember_tab("T-1", WorkspaceTab::Changes);
    state.remember_tab("T-2", WorkspaceTab::History);
    state.remember_tab("T-1", WorkspaceTab::Terminal);
    assert_eq!(state.tab_of("T-1"), Some(WorkspaceTab::Terminal));
    assert_eq!(state.tab_of("T-2"), Some(WorkspaceTab::History));
    assert_eq!(ViewState::parse(&state.to_json()).tabs, state.tabs);
}

#[test]
fn does_not_remember_the_editor_tab_which_needs_an_open_file() {
    let mut state = ViewState::default();
    state.remember_tab("T-1", WorkspaceTab::Terminal);
    state.remember_tab("T-1", WorkspaceTab::Editor);
    assert_eq!(state.tab_of("T-1"), Some(WorkspaceTab::Terminal));
}

#[test]
fn keeps_a_remembered_docs_tab_after_a_reload() {
    let state = ViewState::parse(r#"{"tabs": {"T-0001": "docs"}}"#);
    assert_eq!(state.tab_of("T-0001"), Some(WorkspaceTab::Docs));
    assert_eq!(ViewState::parse(&state.to_json()).tabs, state.tabs);
}

#[test]
fn opens_the_details_of_a_task_on_its_files_and_remembers_that_view() {
    let mut state = ViewState::default();
    let open = state.show_files(&Vec::new(), "T-0003");
    assert_eq!(open, vec![("T-0003".into(), true)]);
    assert_eq!(state.details_view, Some(DetailsView::Files));
    assert_eq!(
        ViewState::parse(&state.to_json()).details_view,
        Some(DetailsView::Files)
    );
}

#[test]
fn keeps_the_docs_view_mode_and_starts_on_rendered() {
    assert!(!ViewState::default().docs_source);
    assert!(ViewState::parse(r#"{"docsSource": true}"#).docs_source);
    assert!(!ViewState::parse(r#"{"docsSource": "source"}"#).docs_source);
}

#[test]
fn follows_the_tab_that_the_user_picks_in_the_open_workspace_only() {
    use crate::workspaces::workspace_rules::shown_after_remember;
    let mut view = ViewState::default();
    let open = Some("T-0001");
    let shown = shown_after_remember(
        open,
        WorkspaceTab::Terminal,
        "T-0001",
        WorkspaceTab::History,
    );
    view.remember_tab("T-0001", WorkspaceTab::History);
    assert_eq!(shown, WorkspaceTab::History);
    let shown = shown_after_remember(open, shown, "T-0002", WorkspaceTab::Changes);
    view.remember_tab("T-0002", WorkspaceTab::Changes);
    assert_eq!(shown, WorkspaceTab::History);
    assert_eq!(view.tab_of("T-0002"), Some(WorkspaceTab::Changes));
}

#[test]
fn drops_a_remembered_progress_tab_which_is_gone() {
    let view = ViewState::parse(r#"{"tabs":{"T-0001":"progress","T-0002":"history"}}"#);
    assert_eq!(
        view.tabs,
        vec![("T-0002".to_string(), WorkspaceTab::History)]
    );
}

#[test]
fn remembers_no_tab_when_the_stored_tabs_are_broken() {
    for broken in [
        r#"{"tabs":"{oops"}"#,
        r#"{"tabs":[1,2]}"#,
        r#"{"tabs":{"T-0001":3}}"#,
    ] {
        assert!(ViewState::parse(broken).tabs.is_empty(), "{broken}");
    }
}

#[test]
fn keeps_the_progress_layout_in_the_file() {
    let state = ViewState::parse(r#"{"layout": "progress"}"#);
    assert_eq!(state.layout, Layout::Progress);
    assert!(state.to_json().contains(r#""layout": "progress""#));
    let again = ViewState::parse(&state.to_json());
    assert_eq!(again.layout, Layout::Progress);
}
