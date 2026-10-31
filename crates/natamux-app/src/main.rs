#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

//! Natamux native app (docs/plan-rust-migration.md, phase 3): screen 1 (the app shell, the
//! sidebar, the task table and board) and screen 2 (the workspace, its terminals and the Agents
//! grid), on the daemon's HTTP and WebSocket API.

mod agents {
    pub mod agent_choices;
    pub mod limits_rules;
}
mod app {
    pub mod account_menu;
    pub mod app_log;
    pub mod appearance;
    pub mod appearance_rules;
    pub mod asset_table;
    pub mod assets;
    pub mod clock;
    pub mod command_types;
    pub mod command_when;
    pub mod commands;
    pub mod commands_app;
    pub mod commands_files;
    pub mod commands_git;
    pub mod commands_stages;
    pub mod dashboard;
    pub mod dashboard_changes;
    pub mod dashboard_data;
    pub mod dock_badge;
    pub mod fake_daemon;
    pub mod flow_bench;
    pub mod gate2;
    pub mod jobs;
    pub mod layout_mode;
    pub mod links;
    pub mod notify;
    pub mod notify_rules;
    pub mod options;
    pub mod read_pace;
    pub mod settings_agents;
    pub mod settings_font;
    pub mod settings_reads;
    pub mod settings_rules;
    pub mod settings_theme;
    pub mod settings_view;
    pub mod sheet_rules;
    pub mod shell_rules;
    pub mod shortcut_sheet;
    pub mod shortcut_text;
    pub mod shortcuts;
    pub mod shots;
    pub mod shots_files;
    pub mod shots_settings;
    pub mod shots_stages;
    pub mod shown_cache;
    pub mod sign_in;
    pub mod sign_in_rules;
    pub mod smoke;
    pub mod source;
    pub mod status_bar;
    pub mod status_items;
    pub mod status_layout;
    pub mod status_limits;
    pub mod tab_rules;
    #[cfg(test)]
    pub mod test_views;
    pub mod text_boxes;
    pub mod theme_catalog;
    pub mod title_bar;
    pub mod update;
    pub mod update_install;
    pub mod update_state;
    pub mod view_rules;
    pub mod view_state;
    pub mod window;
}
mod bench {
    pub mod bench_view;
}
mod components {
    pub mod agent_badge;
    pub mod button_style;
    pub mod chips;
    pub mod colors;
    pub mod confirm;
    pub mod detail_section;
    pub mod dialog;
    pub mod empty_state;
    pub mod form;
    pub mod glyph;
    pub mod icons;
    pub mod input_rules;
    pub mod menu;
    pub mod motion;
    pub mod motion_rules;
    pub mod resize_handle;
    pub mod resize_rules;
    pub mod scroll_rules;
    pub mod segmented;
    pub mod shape;
    pub mod skeleton;
    pub mod spinner;
    pub mod tab_order;
    pub mod terminals_button;
    pub mod text_size;
    pub mod theme_contrast;
    pub mod timing_rules;
    pub mod toast_rules;
    pub mod toasts;
}
mod daemon {
    pub mod health_proof;
    pub mod process;
    #[cfg(test)]
    mod process_tests;
    pub mod start_async;
    #[cfg(test)]
    mod start_async_tests;
}
mod files {
    pub mod draft_store;
    pub mod explorer_tree;
    pub mod file_drafts;
    pub mod file_editor;
    pub mod file_editor_body;
    pub mod file_icon_view;
    pub mod file_icons;
    pub mod file_reads;
    pub mod file_tab_strip;
    pub mod file_tree;
    pub mod file_tree_rows;
    pub mod file_writes;
    pub mod open_files;
}
mod git {
    pub mod change_rules;
    pub mod change_tree;
    pub mod changes_view;
    pub mod comment_rules;
    pub mod commit_rules;
    pub mod diff_cursor;
    pub mod diff_editors;
    pub mod diff_rules;
    pub mod diff_view;
    pub mod graph_lanes;
    pub mod graph_layout;
    pub mod history_view;
    pub mod image_rules;
    pub mod languages;
    pub mod review_blocks;
    pub mod review_rules;
    pub mod rollback_dialog;
    pub mod rollback_rules;
}
mod ime {
    pub mod ime_check;
    pub mod word_check;
}
mod palette {
    pub mod fuzzy;
    pub mod jump_rows;
    pub mod palette_layer;
    pub mod palette_pick;
    pub mod palette_row;
    pub mod palette_rows;
    pub mod palette_view;
}
mod perf {
    pub mod draws;
    pub mod overlay;
    pub mod report;
    pub mod stats;
}
mod projects {
    pub mod branch_name_field;
    pub mod idle_pause_field;
    pub mod issue_import;
    pub mod issue_rules;
    pub mod issues_tab;
    pub mod json_tab;
    pub mod project_rules;
    pub mod project_settings_page;
    pub mod projects_dialog;
    pub mod repo_trust_dialog;
    pub mod send_backs_field;
    pub mod settings_header;
    pub mod settings_tab_rules;
    pub mod settings_tabs;
    pub mod settings_text;
    pub mod settings_writes;
    pub mod welcome;
    pub mod welcome_rules;
}
mod sidebar {
    pub mod blur_bar;
    pub mod comet;
    pub mod project_buttons;
    pub mod project_color;
    pub mod project_header;
    pub mod row_menu;
    pub mod shell_rows;
    pub mod sidebar;
    pub mod sidebar_text;
    pub mod sidebar_tree;
    pub mod tree_entries;
    pub mod tree_model;
    pub mod tree_rename;
    pub mod workspace_groups;
    pub mod workspace_row;
    pub mod workspace_tree;
}
mod stages {
    pub mod branch_details;
    pub mod branch_details_fields;
    pub mod card_rules;
    pub mod editor_branches;
    pub mod editor_catalog;
    pub mod editor_draft;
    pub mod editor_fields;
    pub mod editor_find;
    #[cfg(test)]
    pub mod editor_fixtures;
    pub mod editor_labels;
    pub mod editor_moves;
    pub mod kind_icon;
    pub mod overview_rules;
    pub mod pipeline_editor;
    pub mod pipeline_preview;
    pub mod pipeline_preview_branch;
    pub mod preview_rules;
    pub mod progress_board;
    pub mod progress_board_rules;
    pub mod progress_tab;
    pub mod prompt_field;
    pub mod send_back_dialog;
    pub mod send_back_rules;
    pub mod skill_rules;
    pub mod stage_card;
    pub mod stage_choices;
    pub mod stage_details;
    pub mod stage_details_fields;
    pub mod stage_fields_view;
    pub mod stage_list;
    pub mod stage_list_menus;
    pub mod stage_marker;
    pub mod stage_options;
    pub mod stage_picker;
    pub mod stage_rows;
    pub mod stage_view_rules;
}

mod tasks {
    pub mod add_subtask_dialog;
    pub mod add_task_dialog;
    pub mod delete_finished;
    pub mod details_commands;
    pub mod details_panel;
    pub mod details_rail;
    pub mod details_rules;
    pub mod edit_form;
    pub mod edit_rules;
    pub mod fan_out_dialog;
    pub mod frozen_order;
    pub mod state_filter;
    pub mod state_filter_rules;
    pub mod table_columns;
    pub mod table_menu;
    pub mod table_rules;
    pub mod task_attention;
    pub mod task_board;
    pub mod task_changes;
    pub mod task_confirms;
    pub mod task_events_rules;
    pub mod task_rules;
    pub mod task_sort;
    pub mod task_table;
    pub mod task_table_cells;
    pub mod task_time;
    pub mod task_view;
    pub mod task_view_rules;
    pub mod tries_rules;
    pub mod usage_rules;
}
mod terminals {
    pub mod agent_panes;
    pub mod find_rules;
    pub mod pane_drop;
    pub mod pane_facts;
    pub mod pane_grid;
    pub mod pane_header;
    pub mod pane_rules;
    pub mod paste_rules;
    pub mod remote_terminal;
    pub mod shell_panes;
    pub mod terminal_cache;
    pub mod terminal_find;
    pub mod terminal_font;
    pub mod terminal_pane;
    pub mod terminal_parser;
    pub mod terminal_screen;
    pub mod terminal_size;
    pub mod terminal_tabs;
}
mod workspaces {
    pub mod branches;
    pub mod docs_list;
    pub mod docs_markdown;
    pub mod docs_rules;
    pub mod docs_tab;
    pub mod docs_view;
    pub mod docs_writes;
    pub mod finished_banner;
    pub mod finished_rules;
    pub mod finished_summary;
    pub mod new_workspace_dialog;
    pub mod new_workspace_rules;
    pub mod no_workspace;
    pub mod remove_dialog_rules;
    pub mod remove_finished;
    pub mod remove_finished_rules;
    pub mod remove_workspace_dialog;
    pub mod workspace_header;
    pub mod workspace_rules;
    pub mod workspace_view;
    pub mod wrap_up;
    pub mod wrap_up_rules;
}

use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{App, WindowHandle};

use app::assets::NatamuxAssets;
use app::dashboard::SavedView;
use app::options::{Options, USAGE};
use app::sign_in::StartProblem;
use app::source::Source;
use app::view_state::{self, ViewState};
use app::window::Root;
use daemon::process::DaemonSlot;
use daemon::start_async::{self, Started};
use natamux_client::{Client, ClientError, Config};

fn main() -> ExitCode {
    perf::report::mark_start();
    daemon::process::make_folder_private();
    perf::report::timed("log_trim_ms", app::app_log::trim_at_start);
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let fake = options.fake_daemon || options.shot.is_some();
    let owns_daemon = !fake && std::env::var_os("NATAMUX_URL").is_none();
    let owned = Arc::new(Mutex::new(DaemonSlot::default()));
    // The daemon starts beside the fonts and the window, not before them (T-0221 1.1).
    let started = owns_daemon.then(|| start_async::begin(Arc::clone(&owned)));
    let run_owned = Arc::clone(&owned);
    let failed = Arc::new(AtomicBool::new(false));
    let start_failed = Arc::clone(&failed);
    gpui_platform::application()
        .with_assets(NatamuxAssets)
        .run(move |cx| {
            if let Err(error) = start(&options, started, run_owned, &start_failed, cx) {
                eprintln!("natamux-app: {error:#}");
                start_failed.store(true, Ordering::Relaxed);
                // On Linux a quit asked before the event loop runs is lost: ask from a task.
                cx.spawn(async move |cx| cx.update(|cx| cx.quit())).detach();
            }
        });
    app::update_install::finish_quit(&owned);
    if failed.load(Ordering::Relaxed) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// `started`: the app's own daemon, still starting. `failed`: set by a failed `--smoke` start.
fn start(
    options: &Options,
    started: Option<Started>,
    owned: Arc<Mutex<DaemonSlot>>,
    failed: &Arc<AtomicBool>,
    cx: &mut App,
) -> anyhow::Result<()> {
    perf::report::timed("fonts_ms", || NatamuxAssets.load_fonts(cx))?;
    release_channel::init(
        release_channel::AppVersion::load(env!("CARGO_PKG_VERSION"), None, None),
        cx,
    );
    settings::init(cx);
    // The theme, the UI size and the terminal font of view.json apply before the first frame.
    let fake = options.fake_daemon || options.shot.is_some();
    let folder = view_state::folder().filter(|_| !fake);
    let view = match &folder {
        Some(folder) => perf::report::timed("view_state_ms", || view_state::load(folder)),
        None => ViewState::default(),
    };
    app::appearance::init(options.theme, options.shot.is_some(), cx);
    app::appearance::apply(&view, cx);
    perf::report::timed("themes_ms", || {
        theme_settings::init(theme::LoadThemes::All(Box::new(NatamuxAssets)), cx)
    });
    editor::init(cx);
    git::languages::init(cx);
    app::shortcuts::bind(cx);
    app::shortcuts::set_menus(cx);
    perf::overlay::init(cx);
    components::motion::init(options.shot.is_some(), cx);

    // One window: closing it quits. With a bench running, the window quits first instead
    // (app/window.rs), so the quit handler that ends the bench's shells still finds it.
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    if let Some(shot) = options.shot {
        app::shots::prepare(shot);
    }
    if options.fake_daemon && options.open_task.is_some() {
        app::fake_daemon::shared().clear_jobs();
    }
    let quit_owned = Arc::clone(&owned);
    cx.on_app_quit(move |_| {
        app::update_install::finish_quit(&quit_owned);
        async {}
    })
    .detach();
    let source = source(options, started.is_some());
    let window = app::window::open(options, source, SavedView { view, folder }, cx)?;
    app::update::set_window(window, cx);
    if !fake && !options.smoke {
        app::update::start_timers(cx);
    }
    if let Some(started) = started {
        let smoke = options.smoke.then(|| Arc::clone(failed));
        start_async::follow(started, window, owned, smoke, cx);
    }
    window.update(cx, |_, window, _| {
        window.on_next_frame(|_, _| perf::report::mark_first_frame())
    })?;
    run_modes(options, window, cx);
    cx.activate(true);
    Ok(())
}

/// The CI and bench modes of the flags, on the open window.
fn run_modes(options: &Options, window: WindowHandle<Root>, cx: &mut App) {
    if let Some(shot) = options.shot {
        app::shots::run(window, shot, cx);
    }
    if options.smoke {
        app::smoke::run(
            window,
            options.hold_seconds.is_none(),
            options.fake_daemon,
            cx,
        );
    }
    if let Some(task) = &options.gate2 {
        app::gate2::run(
            window,
            task.clone(),
            options.gate2_terminals,
            options.gate2_output,
            cx,
        );
    }
    if options.bench_flows {
        app::flow_bench::run(window, options.bench_tasks, cx);
    }
    if options.count_draws {
        perf::draws::start(window, cx);
    }
    if let Some(task) = &options.open_task {
        perf::draws::open_task(window, task.clone(), cx);
    }
    if let Some(seconds) = options.hold_seconds {
        let smoke_window = options.smoke.then_some(window);
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            cx.update(|cx| {
                if let Some(window) = smoke_window {
                    let _ = window.update(cx, |_, window, _| window.remove_window());
                } else {
                    cx.quit();
                }
            });
        })
        .detach();
    }
}

/// The daemon, or why the app has none: the window then asks for a token, or says why.
/// `owns_daemon`: the app's own daemon still starts; `start_async::follow` hands it over.
fn source(options: &Options, owns_daemon: bool) -> Result<Arc<Source>, StartProblem> {
    // A shot never reads a real daemon.
    if options.fake_daemon || options.shot.is_some() {
        return Ok(Arc::new(Source::FakeDaemon));
    }
    if owns_daemon {
        return Err(StartProblem::Starting);
    }
    let access = app::sign_in::access_file();
    let client = match Config::from_env_with(access.as_deref()) {
        Err(ClientError::NoToken) => return Err(StartProblem::SignIn),
        config => config.and_then(Client::new),
    };
    client
        .map(|client| Arc::new(Source::Daemon(Arc::new(client))))
        .map_err(|error| StartProblem::Down(error.to_string()))
}
