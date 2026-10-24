use super::check::InstallKind;
use super::install::{InstallInput, install_kind, mac_bundle};

#[test]
fn mac_install_requires_a_writable_app_bundle() {
    assert_eq!(mac_bundle("/Applications/Natamux.app/Contents/MacOS/natamux-app"), Some("/Applications/Natamux.app"));
    assert_eq!(install_kind(InstallInput { platform: "macos", executable: "/Applications/Natamux.app/Contents/MacOS/natamux-app", appimage: None, writable: true }), InstallKind::Mac);
    assert_eq!(install_kind(InstallInput { platform: "macos", executable: "/Volumes/Natamux/Natamux.app/Contents/MacOS/natamux-app", appimage: None, writable: true }), InstallKind::Manual);
    assert_eq!(install_kind(InstallInput { platform: "macos", executable: "/Applications/Natamux.app/Contents/MacOS/natamux-app", appimage: None, writable: false }), InstallKind::Manual);
}

#[test]
fn linux_install_kind_follows_the_packaging() {
    assert_eq!(install_kind(InstallInput { platform: "linux", executable: "/tmp/.mount/Natamux", appimage: Some("/home/a/Natamux.AppImage"), writable: true }), InstallKind::AppImage);
    assert_eq!(install_kind(InstallInput { platform: "linux", executable: "/tmp/.mount/Natamux", appimage: Some("/home/a/Natamux.AppImage"), writable: false }), InstallKind::Manual);
    assert_eq!(install_kind(InstallInput { platform: "linux", executable: "/opt/Natamux/natamux-app", appimage: None, writable: false }), InstallKind::Deb);
    assert_eq!(install_kind(InstallInput { platform: "windows", executable: "C:\\Natamux\\natamux-app.exe", appimage: None, writable: true }), InstallKind::Manual);
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
    let folder = std::env::temp_dir().join(format!("natamux-appimage-failed-{}", std::process::id()));
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
