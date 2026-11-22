use super::check::InstallKind;
use super::install::{InstallInput, appimage_for, install_kind, mac_bundle};
use std::path::Path;

#[test]
fn mac_install_requires_a_writable_app_bundle() {
    assert_eq!(
        mac_bundle("/Applications/Natamux.app/Contents/MacOS/natamux-app"),
        Some("/Applications/Natamux.app")
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "macos",
            executable: "/Applications/Natamux.app/Contents/MacOS/natamux-app",
            appimage: None,
            writable: true,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::Mac
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "macos",
            executable: "/Volumes/Natamux/Natamux.app/Contents/MacOS/natamux-app",
            appimage: None,
            writable: true,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::Manual
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "macos",
            executable: "/Applications/Natamux.app/Contents/MacOS/natamux-app",
            appimage: None,
            writable: false,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::Manual
    );
}

#[test]
fn linux_install_kind_follows_the_packaging() {
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/tmp/.mount/Natamux",
            appimage: Some("/home/a/Natamux.AppImage"),
            writable: true,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::AppImage
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/tmp/.mount/Natamux",
            appimage: Some("/home/a/Natamux.AppImage"),
            writable: false,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::Manual
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/opt/Natamux/natamux-app",
            appimage: None,
            writable: false,
            uninstaller: false,
            tarball: false,
        }),
        InstallKind::Deb
    );
}

fn kind(
    platform: &str,
    executable: &str,
    writable: bool,
    uninstaller: bool,
    tarball: bool,
) -> InstallKind {
    install_kind(InstallInput {
        platform,
        executable,
        appimage: None,
        writable,
        uninstaller,
        tarball,
    })
}

#[test]
fn windows_updates_itself_only_from_a_writable_installer_folder() {
    let exe = "C:\\Users\\a\\AppData\\Local\\Programs\\Natamux\\natamux-app.exe";
    assert_eq!(
        kind("windows", exe, true, true, false),
        InstallKind::Windows
    );
    // A copied exe without the uninstaller.
    assert_eq!(
        kind("windows", exe, true, false, false),
        InstallKind::Manual
    );
    assert_eq!(
        kind("windows", exe, false, true, false),
        InstallKind::Manual
    );
}

#[test]
fn an_unpacked_tarball_updates_itself_when_its_folder_is_writable() {
    let exe = "/home/a/natamux/natamux-app";
    assert_eq!(kind("linux", exe, true, false, true), InstallKind::Tarball);
    assert_eq!(kind("linux", exe, false, false, true), InstallKind::Manual);
    assert_eq!(kind("linux", exe, true, false, false), InstallKind::Manual);
    // The deb and the AppImage keep their kinds.
    assert_eq!(
        kind("linux", "/opt/Natamux/natamux-app", false, false, true),
        InstallKind::Deb
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/tmp/.mount/natamux-app",
            appimage: Some("/home/a/Natamux.AppImage"),
            writable: true,
            uninstaller: false,
            tarball: true,
        }),
        InstallKind::AppImage
    );
    assert_eq!(kind("macos", exe, true, false, true), InstallKind::Manual);
}

#[test]
fn windows_install_script_quotes_paths_and_checks_the_hash_first() {
    let script = super::install::windows_install_script(
        "C:\\Users\\O'Neil\\updates\\Natamux-win-x64.exe",
        "abc123",
        "C:\\Users\\O\u{2019}Neil\\Programs\\Natamux",
        "C:\\Users\\O\u{2019}Neil\\Programs\\Natamux\\natamux-app.exe",
        "C:\\Users\\O'Neil\\logs\\update.log",
        true,
    );
    assert!(script.contains("$installer = 'C:\\Users\\O''Neil\\updates\\Natamux-win-x64.exe'"));
    assert!(script.contains("$dir = 'C:\\Users\\O\u{2019}\u{2019}Neil\\Programs\\Natamux'"));
    assert!(script.contains("WaitForExit(120000)"));
    let hash = script.find("Get-FileHash").expect("hash check");
    let install = script
        .find("'/S /D=' + $dir")
        .expect("silent install, /D= last");
    assert!(hash < install);
    assert!(script.contains(&format!("$code = {}", super::install::HASH_MISMATCH_EXIT)));
    // PowerShell 5.1 reads a script without a BOM in the ANSI code page.
    assert!(script.starts_with('\u{FEFF}'));
    // A hash mismatch or a thrown error still reopens the old app.
    let mismatch = script
        .find("does not match its hash")
        .expect("hash mismatch branch");
    let caught = script
        .find("} catch {\r\n$code = 1")
        .expect("caught errors");
    let reopen = script.find("Start-Process -FilePath $app").expect("reopen");
    assert!(mismatch < reopen && caught < reopen);
    assert!(script.contains("update failed: $($_.Exception.Message)"));
    // The app still runs when it did not stop, so that path ends before the reopen.
    let timeout = script
        .find("Note 'app did not stop'; exit 1")
        .expect("timeout");
    assert!(timeout < mismatch);
    let quiet = super::install::windows_install_script("i", "s", "d", "a", "l", false);
    assert!(quiet.contains("does not match its hash"));
    assert!(!quiet.contains("Start-Process -FilePath $app"));
}

#[test]
fn appimage_swap_replaces_only_a_verified_download() {
    let folder = std::env::temp_dir().join(format!("natamux-appimage-swap-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    let current = folder.join("Natamux.AppImage");
    let verified = folder.join("download.AppImage");
    std::fs::write(&current, b"old").expect("old app");
    std::fs::write(&verified, b"new").expect("new app");
    super::install::replace_appimage(&verified, &current).expect("swap");
    assert_eq!(std::fs::read(&current).expect("current"), b"new");
    assert!(!verified.exists());
    std::fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn appimage_swap_keeps_the_current_app_when_staging_fails() {
    let folder =
        std::env::temp_dir().join(format!("natamux-appimage-failed-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    let current = folder.join("Natamux.AppImage");
    std::fs::write(&current, b"old").expect("old app");
    assert!(super::install::replace_appimage(&folder.join("missing"), &current).is_err());
    assert_eq!(std::fs::read(&current).expect("current"), b"old");
    std::fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn mac_install_script_quotes_paths_and_restores_the_old_bundle_on_failure() {
    let script = super::install::mac_install_script(
        "/Applications/Natamux's App.app",
        "/tmp/new/Natamux.app",
        "/tmp/update.log",
        true,
    );
    assert!(script.contains("Natamux'\\''s App.app"));
    assert!(script.contains("mv \"$backup\" \"$current\""));
    assert!(script.contains("while kill -0"));
    assert!(script.contains("open \"$current\""));
}

#[test]
fn mac_normal_quit_installs_without_reopening_the_app() {
    let script = super::install::mac_install_script(
        "/Applications/Natamux.app",
        "/tmp/new/Natamux.app",
        "/tmp/update.log",
        false,
    );
    assert!(script.contains("mv \"$staged\" \"$current\""));
    assert!(!script.contains("open \"$current\""));
}

#[test]
fn a_stale_backup_is_found_for_a_bundle_and_an_appimage() {
    let folder = std::env::temp_dir().join(format!("natamux-stale-backup-{}", std::process::id()));
    std::fs::create_dir_all(&folder).expect("folder");
    let bundle = folder.join("Natamux.app");
    let image = folder.join("Natamux.AppImage");
    assert_eq!(super::install::stale_backup(&bundle), None);
    assert_eq!(super::install::stale_backup(&image), None);
    std::fs::write(folder.join("Natamux.app.old"), b"x").expect("bundle backup");
    std::fs::write(folder.join("Natamux.AppImage.natamux-old"), b"x").expect("image backup");
    assert_eq!(
        super::install::stale_backup(&bundle),
        Some(folder.join("Natamux.app.old"))
    );
    assert_eq!(
        super::install::stale_backup(&image),
        Some(folder.join("Natamux.AppImage.natamux-old"))
    );
    std::fs::remove_dir_all(folder).expect("cleanup");
}

#[cfg(unix)]
mod deb_script {
    use sha2::{Digest, Sha256};
    use std::os::unix::fs::PermissionsExt;
    use std::path::Path;
    use std::process::Command;

    /// Runs the script as `sh -c <script> sh <file> <sha> <installer>`, without pkexec.
    /// The installer is a script that writes `ran` beside itself.
    fn run(name: &str, change_after_hash: bool) -> (Option<i32>, bool) {
        let folder =
            std::env::temp_dir().join(format!("natamux-deb-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&folder).expect("folder");
        let file = folder.join("update.deb");
        std::fs::write(&file, b"package").expect("package");
        let sha = format!("{:x}", Sha256::digest(b"package"));
        if change_after_hash {
            std::fs::write(&file, b"changed").expect("change");
        }
        let installer = folder.join("installer.sh");
        let marker = folder.join("ran");
        std::fs::write(
            &installer,
            format!("#!/bin/sh\ntouch '{}'\n", marker.display()),
        )
        .expect("installer");
        std::fs::set_permissions(&installer, std::fs::Permissions::from_mode(0o755)).expect("mode");
        let status = Command::new("sh")
            .arg("-c")
            .arg(super::super::install::deb_install_script())
            .arg("sh")
            .arg(&file)
            .arg(&sha)
            .arg(&installer)
            .status()
            .expect("sh");
        let ran = Path::new(&marker).exists();
        std::fs::remove_dir_all(folder).expect("cleanup");
        (status.code(), ran)
    }

    #[test]
    fn a_matching_file_is_installed() {
        assert_eq!(run("match", false), (Some(0), true));
    }

    #[test]
    fn a_file_changed_after_download_is_refused_before_the_installer_runs() {
        assert_eq!(
            run("changed", true),
            (Some(super::super::install::HASH_MISMATCH_EXIT), false)
        );
    }
}

#[test]
fn appimage_counts_only_when_the_exe_is_under_appdir() {
    let exe = Path::new("/tmp/.mount_abc/usr/bin/natamux-app");
    let image = Some("/home/a/Natamux.AppImage");
    assert_eq!(
        appimage_for(exe, image, Some("/tmp/.mount_abc")),
        Some("/home/a/Natamux.AppImage".to_string())
    );
    // A variable left over from another AppImage.
    assert_eq!(appimage_for(exe, image, Some("/tmp/.mount_other")), None);
    let deb = Path::new("/opt/Natamux/natamux-app");
    assert_eq!(appimage_for(deb, image, Some("/tmp/.mount_abc")), None);
    assert_eq!(appimage_for(exe, image, Some("")), None);
    assert_eq!(appimage_for(exe, image, None), None);
    assert_eq!(appimage_for(exe, None, Some("/tmp/.mount_abc")), None);
}
