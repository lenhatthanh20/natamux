use super::check::InstallKind;
use super::install::{InstallInput, install_kind, mac_bundle};

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
            writable: true
        }),
        InstallKind::Mac
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "macos",
            executable: "/Volumes/Natamux/Natamux.app/Contents/MacOS/natamux-app",
            appimage: None,
            writable: true
        }),
        InstallKind::Manual
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "macos",
            executable: "/Applications/Natamux.app/Contents/MacOS/natamux-app",
            appimage: None,
            writable: false
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
            writable: true
        }),
        InstallKind::AppImage
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/tmp/.mount/Natamux",
            appimage: Some("/home/a/Natamux.AppImage"),
            writable: false
        }),
        InstallKind::Manual
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "linux",
            executable: "/opt/Natamux/natamux-app",
            appimage: None,
            writable: false
        }),
        InstallKind::Deb
    );
    assert_eq!(
        install_kind(InstallInput {
            platform: "windows",
            executable: "C:\\Natamux\\natamux-app.exe",
            appimage: None,
            writable: true
        }),
        InstallKind::Manual
    );
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
