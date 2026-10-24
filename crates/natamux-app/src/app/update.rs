//! Signed desktop updates, run off the window thread.

use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{App, Global, WindowHandle};
use natamux_update::check::{InstallKind, Release};
use natamux_update::install::{InstallInput, install_kind, mac_bundle, mac_install_script};
use natamux_update::preferences;
use natamux_update::signature::RELEASE_SIGNER;
use natamux_update::transport::{CurlFeed, download_asset};
use natamux_update::updater::{CheckOutcome, check_now};

use super::view_state;
use crate::components::toast_rules::ToastTone;
use crate::components::confirm::Confirm;
use crate::components::toasts::{ToastAction, push_toast, toast};

static CHECKING: AtomicBool = AtomicBool::new(false);
static READY: Mutex<Option<Ready>> = Mutex::new(None);
static NEXT_LAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

struct UpdateWindow(WindowHandle<super::window::Root>);

impl Global for UpdateWindow {}

pub fn set_window(window: WindowHandle<super::window::Root>, cx: &mut App) {
    cx.set_global(UpdateWindow(window));
}

#[derive(Clone)]
struct Ready {
    release: Release,
    kind: InstallKind,
    downloaded: PathBuf,
    current: PathBuf,
    folder: PathBuf,
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
        toast("The app settings folder is unavailable.", ToastTone::Error, cx);
        return;
    };
    match preferences::save(&file, !automatic()) {
        Ok(()) => toast(
            if automatic() { "Automatic update checks are on." } else { "Automatic update checks are off." },
            ToastTone::Info,
            cx,
        ),
        Err(error) => toast(&format!("Could not save the update setting: {error}"), ToastTone::Error, cx),
    }
    super::shortcuts::set_menus(cx);
    if automatic() {
        check(false, cx);
    }
}

pub fn ready() -> bool {
    READY.lock().is_ok_and(|slot| slot.is_some())
}

pub fn restart_title() -> String {
    let version = READY.lock().ok().and_then(|slot| slot.as_ref().map(|ready| ready.release.version.clone()));
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
        cx.background_executor().timer(Duration::from_secs(30)).await;
        loop {
            let _ = cx.update(|cx| {
                if automatic() {
                    check(false, cx);
                }
            });
            cx.background_executor().timer(Duration::from_secs(6 * 60 * 60)).await;
        }
    })
    .detach();
}

pub fn check(manual: bool, cx: &mut App) {
    if !packaged() {
        if manual {
            toast("Updates are available in packaged apps.", ToastTone::Info, cx);
        }
        return;
    }
    if CHECKING.swap(true, Ordering::AcqRel) {
        if manual {
            toast("An update check is already running.", ToastTone::Info, cx);
        }
        return;
    }
    if manual {
        toast("Checking for updates…", ToastTone::Info, cx);
    }
    let job = cx.background_executor().spawn(async move { fetch() });
    cx.spawn(async move |cx| {
        let result = job.await;
        CHECKING.store(false, Ordering::Release);
        let _ = cx.update(|cx| show_result(result, manual, cx));
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
    let running = std::env::var("NATAMUX_UPDATE_AS_VERSION")
        .unwrap_or_else(|_| env!("CARGO_PKG_VERSION").into());
    let outcome = check_now(&CurlFeed, &running, kind, arch, RELEASE_SIGNER)?;
    let ready = if let CheckOutcome::Asset { release, name, sha256 } = &outcome {
        let update_folder = folder.join("updates").join(&release.version);
        let file = update_folder.join(name);
        log(&folder, &format!("downloading {name} for {}", release.tag));
        download_asset(&format!("{}{}", release.downloads, name), &file, sha256)?;
        let downloaded = if kind == InstallKind::Mac {
            #[cfg(target_os = "macos")]
            { natamux_update::install::prepare_mac(&file, &update_folder, &release.version)
                .map_err(|error| error.to_string())? }
            #[cfg(not(target_os = "macos"))]
            { file.clone() }
        } else {
            file
        };
        log(&folder, &format!("ready: {}", release.tag));
        Some(Ready { release: release.clone(), kind, downloaded, current: replace, folder })
    } else {
        None
    };
    Ok((outcome, ready))
}

fn show_result(result: Result<(CheckOutcome, Option<Ready>), String>, manual: bool, cx: &mut App) {
    match result {
        Err(error) => {
            if let Some(folder) = view_state::folder() {
                log(&folder, &format!("failed: {error}"));
            }
            if manual {
                toast(&format!("Update check failed: {error}"), ToastTone::Error, cx);
            }
        }
        Ok((CheckOutcome::UpToDate(_), _)) => {
            if manual {
                toast("Natamux is up to date.", ToastTone::Info, cx);
            }
        }
        Ok((CheckOutcome::Manual(release), _)) => {
            let page = release.page.clone();
            let dialog = Confirm::new(
                format!("Natamux {} is available", release.version),
                "Open the signed release page to install this version.",
                "Open the Release Page",
                move |_, cx| cx.open_url(&page),
            )
            .cancel_label("Later");
            offer(dialog, cx);
        }
        Ok((CheckOutcome::Asset { release, .. }, Some(ready))) => {
            if let Ok(mut slot) = READY.lock() {
                *slot = Some(ready);
            }
            super::shortcuts::set_menus(cx);
            offer(
                Confirm::new(
                    format!("Natamux {} is ready", release.version),
                    "Restart to install the signed update.",
                    "Restart Now",
                    |window, cx| window.dispatch_action(Box::new(super::shortcuts::RestartToUpdate), cx),
                )
                .cancel_label("Later"),
                cx,
            );
            let page = release.page;
            push_toast(
                &format!("Natamux {} is ready.", release.version),
                ToastTone::Success,
                Some("Restart to install the update.".into()),
                vec![ToastAction::new("What's New", move |_, cx| cx.open_url(&page))],
                cx,
            );
        }
        Ok(_) => {}
    }
}

fn offer(dialog: Confirm, cx: &mut App) {
    let window = cx.try_global::<UpdateWindow>().map(|global| global.0.clone());
    if let Some(window) = window {
        let _ = window.update(cx, |_, window, cx| dialog.open(window, cx));
    }
}

pub fn restart(cx: &mut App) {
    let ready = READY.lock().ok().and_then(|slot| slot.clone());
    let Some(ready) = ready else {
        toast("There is no update ready to install.", ToastTone::Info, cx);
        return;
    };
    if ready.kind == InstallKind::Deb {
        toast("Installing the update…", ToastTone::Info, cx);
        let job = cx.background_executor().spawn(async move {
            log(&ready.folder, &format!("installing {}", ready.release.tag));
            let status = Command::new("pkexec")
                .arg("dpkg")
                .arg("-i")
                .arg(&ready.downloaded)
                .status();
            (ready, status)
        });
        cx.spawn(async move |cx| {
            let (ready, result) = job.await;
            let _ = cx.update(|cx| match result {
                Ok(status) if status.success() => {
                    log(&ready.folder, "installed update");
                    queue_launch(ready.current);
                    cx.quit();
                }
                _ => {
                    log(&ready.folder, "failed to install the deb package");
                    push_toast(
                        "The package was not installed.",
                        ToastTone::Error,
                        Some(format!("Run: sudo apt install {}", ready.downloaded.display())),
                        Vec::new(),
                        cx,
                    );
                }
            });
        })
        .detach();
        return;
    }
    match install_ready(&ready, true) {
        Ok(()) => {
            if let Ok(mut slot) = READY.lock() {
                *slot = None;
            }
            if ready.kind == InstallKind::AppImage {
                queue_launch(ready.current);
            }
            cx.quit();
        }
        Err(error) => toast(&format!("Could not install the update: {error}"), ToastTone::Error, cx),
    }
}

fn queue_launch(binary: PathBuf) {
    if let Ok(mut next) = NEXT_LAUNCH.lock() {
        *next = Some(binary);
    }
}

pub fn request_restart(cx: &mut App) {
    if let Ok(binary) = std::env::current_exe() {
        queue_launch(binary);
        cx.quit();
    }
}

pub fn take_restart() -> Option<PathBuf> {
    NEXT_LAUNCH.lock().ok().and_then(|mut next| next.take())
}

/// A ready macOS update is also installed after an ordinary quit.
pub fn install_on_quit() {
    let ready = READY.lock().ok().and_then(|mut slot| slot.take());
    if let Some(ready) = ready.filter(|ready| ready.kind == InstallKind::Mac) {
        if let Err(error) = install_ready(&ready, false) {
            log(&ready.folder, &format!("could not install update on quit: {error}"));
        }
    }
}

fn install_ready(ready: &Ready, relaunch: bool) -> io::Result<()> {
    log(&ready.folder, &format!("installing {}", ready.release.tag));
    match ready.kind {
        InstallKind::Mac => {
            let script = ready.folder.join("updates").join(&ready.release.version).join("install.sh");
            let log_file = ready.folder.join("logs/update.log");
            fs::write(&script, mac_install_script(
                &ready.current.to_string_lossy(),
                &ready.downloaded.to_string_lossy(),
                &log_file.to_string_lossy(),
                relaunch,
            ))?;
            Command::new("/bin/sh")
                .arg(script)
                .arg(std::process::id().to_string())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()?;
        }
        InstallKind::AppImage => {
            natamux_update::install::replace_appimage(&ready.downloaded, &ready.current)?;
        }
        InstallKind::Deb => return Err(io::Error::other("The package installer must finish before restart")),
        InstallKind::Manual => return Err(io::Error::other("This installation needs a manual update")),
    }
    Ok(())
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

fn log(folder: &Path, message: &str) {
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
    use super::{queue_launch, restart_title_for, take_restart};

    #[test]
    fn the_restart_menu_names_the_ready_version() {
        assert_eq!(restart_title_for(Some("0.2.0")), "Restart to Update to 0.2.0");
        assert_eq!(restart_title_for(None), "Restart to Update");
    }

    #[test]
    fn a_relaunch_is_taken_once_after_the_app_stops() {
        let binary = std::path::PathBuf::from("/tmp/natamux-app");
        queue_launch(binary.clone());
        assert_eq!(take_restart(), Some(binary));
        assert_eq!(take_restart(), None);
    }
}
