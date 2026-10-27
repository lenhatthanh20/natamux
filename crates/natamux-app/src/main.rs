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
    pub mod gate2;
    pub mod jobs;
    pub mod layout_mode;
    pub mod notify;
    pub mod notify_rules;
    pub mod options;
    pub mod read_pace;
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
    pub mod theme_catalog;
    pub mod title_bar;
    pub mod update;
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
    pub mod process;
    #[cfg(test)]
    mod process_tests;
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
    pub mod file_tree;
    pub mod file_tree_rows;
    pub mod file_writes;
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
    pub mod idle_pause_field;
    pub mod issue_import;
    pub mod issue_rules;
    pub mod issues_tab;
    pub mod json_tab;
    pub mod project_rules;
    pub mod project_settings_page;
    pub mod projects_dialog;
    pub mod send_backs_field;
    pub mod settings_header;
    pub mod settings_tab_rules;
    pub mod settings_tabs;
    pub mod settings_text;
    pub mod settings_writes;
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
    pub mod pane_header;
    pub mod pane_rules;
    pub mod paste_rules;
    pub mod remote_terminal;
    pub mod shell_panes;
    pub mod terminal_cache;
    pub mod terminal_find;
    pub mod terminal_font;
    pub mod terminal_pane;
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
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::App;

use app::assets::NatamuxAssets;
use app::options::{Options, USAGE};
use app::sign_in::StartProblem;
use app::source::Source;
use app::view_state::{self, ViewState};
use components::toast_rules::ToastTone;
use components::toasts::{ToastAction, push_toast};
use daemon::process::OwnedDaemon;
use natamux_client::{Client, ClientError, Config};

fn main() -> ExitCode {
    perf::report::mark_start();
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    let owned = if options.fake_daemon
        || options.shot.is_some()
        || std::env::var_os("NATAMUX_URL").is_some()
    {
        None
    } else {
        Some(OwnedDaemon::start())
    };
    if options.smoke && owned.as_ref().is_some_and(Result::is_err) {
        if let Some(Err(error)) = owned.as_ref() {
            eprintln!("smoke: the app could not start its daemon: {error}");
        }
        return ExitCode::FAILURE;
    }
    let local = owned.as_ref().map(|result| {
        result
            .as_ref()
            .map(OwnedDaemon::config)
            .map_err(std::string::ToString::to_string)
    });
    let child = owned
        .as_ref()
        .and_then(|result| result.as_ref().ok().map(OwnedDaemon::child));
    let owned = Arc::new(Mutex::new(owned.and_then(Result::ok)));
    let run_owned = Arc::clone(&owned);
    let failed = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let start_failed = Arc::clone(&failed);
    gpui_platform::application()
        .with_assets(NatamuxAssets)
        .run(move |cx| {
            if let Err(error) = start(
                &options,
                local.as_ref(),
                child.clone(),
                Arc::clone(&run_owned),
                cx,
            ) {
                eprintln!("natamux-app: {error:#}");
                start_failed.store(true, std::sync::atomic::Ordering::Relaxed);
                // On Linux a quit asked before the event loop runs is lost: ask from a task.
                cx.spawn(async move |cx| cx.update(|cx| cx.quit())).detach();
            }
        });
    app::update::finish_quit(&owned);
    if failed.load(std::sync::atomic::Ordering::Relaxed) {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn start(
    options: &Options,
    local: Option<&Result<Config, String>>,
    child: Option<Arc<std::sync::Mutex<std::process::Child>>>,
    owned: Arc<Mutex<Option<OwnedDaemon>>>,
    cx: &mut App,
) -> anyhow::Result<()> {
    NatamuxAssets.load_fonts(cx)?;
    release_channel::init(
        release_channel::AppVersion::load(env!("CARGO_PKG_VERSION"), None, None),
        cx,
    );
    settings::init(cx);
    // The theme, the UI size and the terminal font of view.json apply before the first frame.
    let fake = options.fake_daemon || options.shot.is_some();
    let view = match view_state::folder() {
        Some(folder) if !fake => view_state::load(&folder),
        _ => ViewState::default(),
    };
    app::appearance::init(options.theme, options.shot.is_some(), cx);
    app::appearance::apply(&view, cx);
    theme_settings::init(theme::LoadThemes::All(Box::new(NatamuxAssets)), cx);
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
    cx.on_app_quit(move |_| {
        app::update::finish_quit(&owned);
        async {}
    })
    .detach();
    let window = app::window::open(options, source(options, local), cx)?;
    app::update::set_window(window, cx);
    if !fake && !options.smoke {
        app::update::start_timers(cx);
    }
    if let Some(child) = child {
        watch_daemon(child, cx);
    }
    window.update(cx, |_, window, _| {
        window.on_next_frame(|_, _| perf::report::mark_first_frame())
    })?;
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
        app::gate2::run(window, task.clone(), cx);
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
    cx.activate(true);
    Ok(())
}

fn watch_daemon(child: Arc<std::sync::Mutex<std::process::Child>>, cx: &mut App) {
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let exited = child.lock().map_or(true, |mut process| {
                process.try_wait().ok().flatten().is_some()
            });
            if exited {
                cx.update(|cx| {
                    push_toast(
                        "The Natamux daemon stopped.",
                        ToastTone::Error,
                        Some("Restart the app to reconnect.".into()),
                        vec![ToastAction::new("Restart", |_, cx| {
                            app::update::request_restart(cx);
                        })],
                        cx,
                    );
                });
                break;
            }
        }
    })
    .detach();
}

/// The daemon, or why the app has none: the window then asks for a token, or says why.
fn source(
    options: &Options,
    local: Option<&Result<Config, String>>,
) -> Result<Arc<Source>, StartProblem> {
    // A shot never reads a real daemon.
    if options.fake_daemon || options.shot.is_some() {
        return Ok(Arc::new(Source::FakeDaemon));
    }
    let client = match local {
        Some(Err(reason)) => return Err(StartProblem::Down(reason.clone())),
        Some(Ok(config)) => Client::new(config.clone()),
        None => {
            let access = app::sign_in::access_file();
            match Config::from_env_with(access.as_deref()) {
                Err(ClientError::NoToken) => return Err(StartProblem::SignIn),
                config => config.and_then(Client::new),
            }
        }
    };
    client
        .map(|client| Arc::new(Source::Daemon(Arc::new(client))))
        .map_err(|error| StartProblem::Down(error.to_string()))
}
