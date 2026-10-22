//! Natamux native app, phase 1 spike (docs/plan-rust-migration.md): the task list and a live
//! agent terminal, on the daemon's HTTP and WebSocket API.

mod app {
    pub mod fake_daemon;
    pub mod options;
    pub mod shortcuts;
    pub mod smoke;
    pub mod source;
    pub mod window;
}
mod bench {
    pub mod bench_view;
}
mod ime {
    pub mod ime_check;
    pub mod word_check;
}
mod perf {
    pub mod overlay;
    pub mod report;
    pub mod stats;
}
mod tasks {
    pub mod task_filter;
    pub mod task_list;
}
mod terminals {
    pub mod remote_terminal;
    pub mod terminal_pane;
}

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use assets::Assets;
use gpui::{App, UpdateGlobal};
use settings::SettingsStore;

use app::options::{Options, ThemeMode, USAGE};
use app::source::Source;
use natamux_client::{Client, Config};

fn main() -> ExitCode {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}\n{USAGE}");
            return ExitCode::FAILURE;
        }
    };
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx| {
            if let Err(error) = start(&options, cx) {
                eprintln!("natamux-app: {error:#}");
                std::process::exit(1);
            }
        });
    ExitCode::SUCCESS
}

fn start(options: &Options, cx: &mut App) -> anyhow::Result<()> {
    Assets.load_fonts(cx)?;
    release_channel::init(
        release_channel::AppVersion::load(env!("CARGO_PKG_VERSION"), None, None),
        cx,
    );
    settings::init(cx);
    if let Some(mode) = options.theme {
        set_theme_mode(mode, cx);
    }
    theme_settings::init(theme::LoadThemes::All(Box::new(Assets)), cx);
    editor::init(cx);
    app::shortcuts::bind(cx);
    app::shortcuts::set_menus(cx);
    perf::overlay::init(cx);

    // One window: closing it quits. With a bench running, the window quits first instead
    // (app/window.rs), so the quit handler that ends the bench's shells still finds it.
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
    let window = app::window::open(options, source(options), cx)?;
    if options.smoke {
        app::smoke::run(window, options.hold_seconds.is_none(), cx);
    }
    if let Some(seconds) = options.hold_seconds {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(Duration::from_secs(seconds))
                .await;
            cx.update(|cx| cx.quit());
        })
        .detach();
    }
    cx.activate(true);
    Ok(())
}

/// The daemon, or why the app has none: the window then says so instead of a list.
fn source(options: &Options) -> Result<Arc<Source>, String> {
    if options.fake_daemon {
        return Ok(Arc::new(Source::FakeDaemon));
    }
    let client = Config::from_env()
        .and_then(Client::new)
        .map_err(|error| error.to_string())?;
    Ok(Arc::new(Source::Daemon(Arc::new(client))))
}

// The flag wins over the system look, as a user setting would.
fn set_theme_mode(mode: ThemeMode, cx: &mut App) {
    let mode = match mode {
        ThemeMode::Light => "light",
        ThemeMode::Dark => "dark",
    };
    let content =
        format!(r#"{{"theme": {{"mode": "{mode}", "light": "One Light", "dark": "One Dark"}}}}"#);
    SettingsStore::update_global(cx, |store, cx| {
        let _ = store.set_user_settings(&content, cx);
    });
}
