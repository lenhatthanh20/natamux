//! Installing a ready update: on Restart to Update, or when the app quits (macOS).

use std::fs;
use std::io;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};

use gpui::App;
use natamux_update::check::InstallKind;
use natamux_update::install::mac_install_script;

use super::update::{READY, Ready, log};
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::{push_toast, toast};
use crate::daemon::process::OwnedDaemon;

static NEXT_LAUNCH: Mutex<Option<PathBuf>> = Mutex::new(None);

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
            cx.update(|cx| match result {
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
                        Some(format!(
                            "Run: sudo apt install {}",
                            ready.downloaded.display()
                        )),
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
        Err(error) => toast(
            &format!("Could not install the update: {error}"),
            ToastTone::Error,
            cx,
        ),
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

/// Finish a ready macOS update and return a queued app relaunch, if it still needs one.
pub fn finish_on_quit() -> Option<PathBuf> {
    let launch = take_restart();
    let ready = READY.lock().ok().and_then(|mut slot| slot.take());
    if let Some(ready) = ready.filter(|ready| ready.kind == InstallKind::Mac) {
        match install_ready(&ready, launch.is_some()) {
            Ok(()) => return None,
            Err(error) => log(
                &ready.folder,
                &format!("could not install update on quit: {error}"),
            ),
        }
    }
    launch
}

pub fn finish_quit(owned: &Arc<Mutex<Option<OwnedDaemon>>>) {
    let launch = finish_on_quit();
    if let Ok(mut slot) = owned.lock() {
        let daemon = slot.take();
        drop(slot);
        drop(daemon);
    }
    if let Some(binary) = launch
        && let Err(error) = Command::new(binary).spawn()
    {
        eprintln!("natamux-app: could not restart: {error}");
    }
}

fn install_ready(ready: &Ready, relaunch: bool) -> io::Result<()> {
    log(&ready.folder, &format!("installing {}", ready.release.tag));
    match ready.kind {
        InstallKind::Mac => {
            let script = ready
                .folder
                .join("updates")
                .join(&ready.release.version)
                .join("install.sh");
            let log_file = ready.folder.join("logs/update.log");
            fs::write(
                &script,
                mac_install_script(
                    &ready.current.to_string_lossy(),
                    &ready.downloaded.to_string_lossy(),
                    &log_file.to_string_lossy(),
                    relaunch,
                ),
            )?;
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
        InstallKind::Deb => {
            return Err(io::Error::other(
                "The package installer must finish before restart",
            ));
        }
        InstallKind::Manual => {
            return Err(io::Error::other("This installation needs a manual update"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{finish_on_quit, queue_launch, take_restart};

    #[test]
    fn a_relaunch_is_taken_once_after_the_app_stops() {
        let binary = std::path::PathBuf::from("/tmp/natamux-app");
        queue_launch(binary.clone());
        assert_eq!(finish_on_quit(), Some(binary));
        assert_eq!(take_restart(), None);
    }
}
