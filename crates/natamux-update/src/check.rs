//! Stable release addresses, versions, assets, and checksum lines.

pub const RELEASE_ROOT: &str = "https://github.com/lenhatthanh20/natamux";
pub const LATEST_URL: &str = "https://github.com/lenhatthanh20/natamux/releases/latest";
pub const SUMS_FILE: &str = "SHA256SUMS.txt";
pub const SIGNATURE_FILE: &str = "SHA256SUMS.txt.minisig";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub tag: String,
    pub version: String,
    pub page: String,
    pub downloads: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InstallKind {
    Mac,
    AppImage,
    Deb,
    Manual,
}

pub fn parse_version(text: &str) -> Option<[u64; 3]> {
    let version = text.trim().strip_prefix('v').unwrap_or(text.trim());
    let mut parts = version.split('.');
    let mut read = || {
        let part = parts.next()?;
        (!part.is_empty()
            && (part == "0" || !part.starts_with('0'))
            && part.bytes().all(|byte| byte.is_ascii_digit()))
            .then(|| part.parse::<u64>().ok())
            .flatten()
    };
    let result = [read()?, read()?, read()?];
    parts.next().is_none().then_some(result)
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (parse_version(candidate), parse_version(current)) {
        (Some(next), Some(now)) => next > now,
        _ => false,
    }
}

pub fn release_from_url(url: &str) -> Option<Release> {
    let rest = url.strip_prefix(RELEASE_ROOT)?.strip_prefix('/')?;
    let parts: Vec<&str> = rest.split('/').collect();
    let ["releases", "tag", tag] = parts.as_slice() else {
        return None;
    };
    let version = parse_version(tag)?;
    Some(Release {
        tag: (*tag).to_string(),
        version: format!("{}.{}.{}", version[0], version[1], version[2]),
        page: url.to_string(),
        downloads: format!("{RELEASE_ROOT}/releases/download/{tag}/"),
    })
}

pub fn asset_name(kind: InstallKind, arch: &str) -> Option<&'static str> {
    match (kind, arch) {
        (InstallKind::Mac, "arm64") => Some("Natamux-mac-arm64.dmg"),
        (InstallKind::Mac, "x64") => Some("Natamux-mac-x64.dmg"),
        (InstallKind::AppImage, "x64") => Some("Natamux-linux-x86_64.AppImage"),
        (InstallKind::Deb, "x64") => Some("Natamux-linux-amd64.deb"),
        _ => None,
    }
}

pub fn sha256_from(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let line = line.trim_end_matches('\r');
        let (hash, file) = line.split_once(' ')?;
        let file = file.strip_prefix(' ').or_else(|| file.strip_prefix('*'))?;
        (file == name && hash.len() == 64 && hash.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| hash.to_ascii_lowercase())
    })
}
