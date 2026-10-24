use super::check::{InstallKind, asset_name, is_newer, release_from_url, sha256_from};

#[test]
fn stable_versions_compare_each_part() {
    assert!(is_newer("v1.10.0", "1.9.9"));
    assert!(!is_newer("v1.2.0-beta.1", "1.1.0"));
    assert!(!is_newer("v1.2.0", "1.2.0"));
    assert!(!is_newer("bad", "1.0.0"));
    assert!(super::check::parse_version("v01.2.3").is_none());
}

#[test]
fn only_a_stable_github_release_redirect_is_used() {
    let release = release_from_url("https://github.com/lenhatthanh20/natamux/releases/tag/v1.2.3")
        .expect("release");
    assert_eq!(release.tag, "v1.2.3");
    assert_eq!(release.version, "1.2.3");
    assert_eq!(
        release.downloads,
        "https://github.com/lenhatthanh20/natamux/releases/download/v1.2.3/"
    );
    assert!(release_from_url("https://github.com/x/y/releases/latest").is_none());
    assert!(release_from_url("https://github.com/x/y/releases/tag/v1.2.3").is_none());
    assert!(release_from_url("https://github.com/x/y/releases/tag/v1.2.3-beta").is_none());
}

#[test]
fn keeps_the_asset_names_of_the_last_electron_updater() {
    assert_eq!(asset_name(InstallKind::Mac, "arm64"), Some("Natamux-mac-arm64.dmg"));
    assert_eq!(asset_name(InstallKind::Mac, "x64"), Some("Natamux-mac-x64.dmg"));
    assert_eq!(asset_name(InstallKind::AppImage, "x64"), Some("Natamux-linux-x86_64.AppImage"));
    assert_eq!(asset_name(InstallKind::Deb, "x64"), Some("Natamux-linux-amd64.deb"));
    assert_eq!(asset_name(InstallKind::Manual, "x64"), None);
}

#[test]
fn takes_only_the_named_hash_from_signed_sums() {
    let hash = "a".repeat(64);
    let sums = format!("{hash}  other.dmg\n{hash} *Natamux-mac-arm64.dmg\n");
    assert_eq!(sha256_from(&sums, "Natamux-mac-arm64.dmg"), Some(hash));
    assert_eq!(sha256_from(&sums, "missing.dmg"), None);
    assert_eq!(sha256_from("bad  file", "file"), None);
}
