//! Signed desktop updates, run off the window thread.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use gpui::{App, Global, WindowHandle};
use natamux_update::check::{InstallKind, Release};
use natamux_update::install::{InstallInput, install_kind, mac_bundle};
use natamux_update::preferences;
use natamux_update::signature::RELEASE_SIGNER;
use natamux_update::transport::{CurlFeed, download_asset};
use natamux_update::updater::{CheckOutcome, check_now};

use super::update_state::{CheckStart, UpdateState, ago};
use super::view_state;
use crate::components::confirm::Confirm;
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{ToastAction, push_toast, toast};

static STATE: Mutex<UpdateState> = Mutex::new(UpdateState::Idle);
/// Bytes of the running download; the views read it on each draw.
static BYTES: AtomicU64 = AtomicU64::new(0);
static LAST_CHECKED: Mutex<Option<Instant>> = Mutex::new(None);
pub(super) static READY: Mutex<Option<Ready>> = Mutex::new(None);
/// How often the views draw a running download again.
const PROGRESS_EVERY: Duration = Duration::from_secs(1);

struct UpdateWindow(WindowHandle<super::window::Root>);

impl Global for UpdateWindow {}

/// Set again on every change of the update state: views that show it observe this global.
pub struct UpdateChanged;

impl Global for UpdateChanged {}

pub fn state() -> UpdateState {
    STATE.lock().map(|state| state.clone()).unwrap_or_default()
}

pub fn bytes() -> u64 {
    BYTES.load(Ordering::Relaxed)
}

/// When the last check ended, as "3m ago"; None before the first one.
pub fn checked_ago() -> Option<String> {
    let at = LAST_CHECKED.lock().ok().and_then(|at| *at)?;
    Some(ago(at.elapsed().as_secs()))
}

pub fn running_version() -> String {
    std::env::var("NATAMUX_UPDATE_AS_VERSION").unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into())
}

fn store(state: UpdateState) {
    if let Ok(mut slot) = STATE.lock() {
        *slot = state;
    }
}

/// Stores the state, then draws the menus and the views that show it.
fn set_state(state: UpdateState, cx: &mut App) {
    store(state);
    changed(cx);
}

fn changed(cx: &mut App) {
    cx.set_global(UpdateChanged);
    super::shortcuts::set_menus(cx);
}

/// A `--shot` state: the update shows as if a check got there.
pub fn show_for_shot(state: UpdateState, bytes: u64, cx: &mut App) {
    BYTES.store(bytes, Ordering::Relaxed);
    set_state(state, cx);
}

pub fn set_window(window: WindowHandle<super::window::Root>, cx: &mut App) {
    cx.set_global(UpdateWindow(window));
}

#[derive(Clone)]
pub(super) struct Ready {
    pub(super) release: Release,
    pub(super) kind: InstallKind,
    pub(super) downloaded: PathBuf,
    pub(super) current: PathBuf,
    pub(super) folder: PathBuf,
}

fn settings_file() -> Option<PathBuf> {
    view_state::folder().map(|folder| folder.join("settings.json"))
}

pub fn automatic() -> bool {
    settings_file()
        .and_then(|file| preferences::load(&file).ok())
        .unwrap_or(false)
}

pub fn toggle_automatic(cx: &mut App) {
    let Some(file) = settings_file() else {
        toast(
            "The app settings folder is unavailable.",
            ToastTone::Error,
            cx,
        );
        return;
    };
    match preferences::save(&file, !automatic()) {
        Ok(()) => toast(
            if automatic() {
                "Automatic update checks are on."
            } else {
                "Automatic update checks are off."
            },
            ToastTone::Info,
            cx,
        ),
        Err(error) => toast(
            &format!("Could not save the update setting: {error}"),
            ToastTone::Error,
            cx,
        ),
    }
    super::shortcuts::set_menus(cx);
    if automatic() {
        check(false, cx);
    }
}

pub fn ready() -> bool {
    READY.lock().is_ok_and(|slot| slot.is_some())
}

pub fn busy() -> bool {
    state().busy()
}

pub fn restart_title() -> String {
    let version = READY
        .lock()
        .ok()
        .and_then(|slot| slot.as_ref().map(|ready| ready.release.version.clone()));
    restart_title_for(version.as_deref())
}

fn restart_title_for(version: Option<&str>) -> String {
    version.map_or_else(
        || "Restart to Update".into(),
        |version| format!("Restart to Update to {version}"),
    )
}

pub fn start_timers(cx: &mut App) {
    if !packaged() {
        return;
    }
    cx.spawn(async move |cx| {
        cx.background_executor()
            .timer(Duration::from_secs(30))
            .await;
        loop {
            cx.update(|cx| {
                if automatic() {
                    check(false, cx);
                }
            });
            cx.background_executor()
                .timer(Duration::from_secs(6 * 60 * 60))
                .await;
        }
    })
    .detach();
}

pub fn check(manual: bool, cx: &mut App) {
    if !packaged() {
        if manual {
            toast(
                "Updates are available in packaged apps.",
                ToastTone::Info,
                cx,
            );
        }
        return;
    }
    let now = state();
    match now.on_check(manual) {
        CheckStart::Skip => return,
        CheckStart::Join => return set_state(now.joined(), cx),
        CheckStart::ShowReady => {
            if let UpdateState::Ready { version } = now {
                offer_ready(&version, cx);
            }
            return;
        }
        CheckStart::Run => {}
    }
    BYTES.store(0, Ordering::Relaxed);
    set_state(UpdateState::Checking { manual }, cx);
    let job = cx.background_executor().spawn(async move { fetch() });
    cx.spawn(async move |cx| {
        let result = job.await;
        cx.update(|cx| show_result(result, cx));
    })
    .detach();
    // A download changes only BYTES, so the views are told on a timer while it runs.
    cx.spawn(async move |cx| {
        loop {
            cx.background_executor().timer(PROGRESS_EVERY).await;
            let running = cx.update(|cx| {
                let running = busy();
                if running {
                    cx.set_global(UpdateChanged);
                }
                running
            });
            if !running {
                break;
            }
        }
    })
    .detach();
}

fn fetch() -> Result<(CheckOutcome, Option<Ready>), String> {
    let folder = view_state::folder().ok_or("The app settings folder is unavailable")?;
    log(&folder, "checking for an update");
    let current = std::env::current_exe().map_err(|error| error.to_string())?;
    let current_text = current.to_string_lossy();
    let appimage = std::env::var("APPIMAGE").ok();
    let replace = if let Some(bundle) = mac_bundle(&current_text) {
        PathBuf::from(bundle)
    } else if let Some(image) = &appimage {
        PathBuf::from(image)
    } else {
        current.clone()
    };
    let writable = replace.parent().is_some_and(|parent| {
        fs::metadata(parent).is_ok_and(|meta| !meta.permissions().readonly())
    });
    let kind = install_kind(InstallInput {
        platform: std::env::consts::OS,
        executable: &current_text,
        appimage: appimage.as_deref(),
        writable,
    });
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        other => other,
    };
    let running = running_version();
    let outcome = check_now(&CurlFeed, &running, kind, arch, RELEASE_SIGNER)?;
    let ready = if let CheckOutcome::Asset {
        release,
        name,
        sha256,
    } = &outcome
    {
        let update_folder = folder.join("updates").join(&release.version);
        let file = update_folder.join(name);
        log(&folder, &format!("downloading {name} for {}", release.tag));
        store(state().downloading(&release.version));
        download_asset(
            &format!("{}{}", release.downloads, name),
            &file,
            sha256,
            |total| BYTES.store(total, Ordering::Relaxed),
        )?;
        let downloaded = if kind == InstallKind::Mac {
            #[cfg(target_os = "macos")]
            {
                natamux_update::install::prepare_mac(&file, &update_folder, &release.version)
                    .map_err(|error| error.to_string())?
            }
            #[cfg(not(target_os = "macos"))]
            {
                file.clone()
            }
        } else {
            file
        };
        log(&folder, &format!("ready: {}", release.tag));
        Some(Ready {
            release: release.clone(),
            kind,
            downloaded,
            current: replace,
            folder,
        })
    } else {
        None
    };
    Ok((outcome, ready))
}

fn show_result(result: Result<(CheckOutcome, Option<Ready>), String>, cx: &mut App) {
    let manual = state().manual();
    if let Ok(mut at) = LAST_CHECKED.lock() {
        *at = Some(Instant::now());
    }
    match result {
        Err(error) => {
            if let Some(folder) = view_state::folder() {
                log(&folder, &format!("failed: {error}"));
            }
            if !manual {
                return set_state(UpdateState::Idle, cx);
            }
            set_state(
                UpdateState::Failed {
                    message: error.clone(),
                },
                cx,
            );
            push_toast(
                "Could not check for updates",
                ToastTone::Error,
                Some(error),
                vec![ToastAction::new("Try Again", |_, cx| check(true, cx))],
                cx,
            );
        }
        Ok((CheckOutcome::Manual(release), _)) => {
            set_state(
                UpdateState::Manual {
                    version: release.version.clone(),
                    page: release.page.clone(),
                },
                cx,
            );
            if manual {
                let page = release.page;
                let dialog = Confirm::new(
                    format!("Natamux {} is available", release.version),
                    "Open the signed release page to install this version.",
                    "Open the Release Page",
                    move |_, cx| cx.open_url(&page),
                )
                .cancel_label("Later");
                offer(dialog, cx);
            }
        }
        Ok((CheckOutcome::Asset { release, .. }, Some(ready))) => {
            if let Ok(mut slot) = READY.lock() {
                *slot = Some(ready);
            }
            set_state(
                UpdateState::Ready {
                    version: release.version.clone(),
                },
                cx,
            );
            if manual {
                offer_ready(&release.version, cx);
                return;
            }
            // A timed check does not stop the work with a dialog.
            let page = release.page;
            push_toast(
                &format!("Natamux {} is ready", release.version),
                ToastTone::Success,
                Some("Restart to install it.".into()),
                vec![
                    ToastAction::new("Restart Now", |window, cx| {
                        window.dispatch_action(Box::new(super::shortcuts::RestartToUpdate), cx)
                    }),
                    ToastAction::new("What's New", move |_, cx| cx.open_url(&page)),
                ],
                cx,
            );
        }
        Ok((CheckOutcome::Asset { .. }, None)) => set_state(UpdateState::Idle, cx),
        Ok((CheckOutcome::UpToDate(_), _)) => {
            set_state(UpdateState::UpToDate, cx);
            if manual {
                toast(
                    &format!("Natamux {} is the latest version.", running_version()),
                    ToastTone::Info,
                    cx,
                );
            }
        }
    }
}

fn offer_ready(version: &str, cx: &mut App) {
    offer(
        Confirm::new(
            format!("Natamux {version} is ready"),
            "Restart to install the signed update.",
            "Restart Now",
            |window, cx| window.dispatch_action(Box::new(super::shortcuts::RestartToUpdate), cx),
        )
        .cancel_label("Later"),
        cx,
    );
}

fn offer(dialog: Confirm, cx: &mut App) {
    let window = cx.try_global::<UpdateWindow>().map(|global| global.0);
    if let Some(window) = window {
        let _ = window.update(cx, |_, window, cx| dialog.open(window, cx));
    }
}

fn packaged() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let text = exe.to_string_lossy();
    mac_bundle(&text).is_some()
        || std::env::var_os("APPIMAGE").is_some()
        || text.starts_with("/opt/Natamux/")
        || cfg!(windows) && exe.with_file_name("natamux-daemon.exe").is_file()
}

pub(super) fn log(folder: &Path, message: &str) {
    let path = folder.join("logs/update.log");
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = writeln!(file, "{message}");
    }
}

#[cfg(test)]
mod tests {
    use super::restart_title_for;

    #[test]
    fn the_restart_menu_names_the_ready_version() {
        assert_eq!(
            restart_title_for(Some("0.2.0")),
            "Restart to Update to 0.2.0"
        );
        assert_eq!(restart_title_for(None), "Restart to Update");
    }
}
