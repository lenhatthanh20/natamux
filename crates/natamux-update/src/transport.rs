//! HTTPS release downloads through the system curl executable.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use natamux_core::egress::{Target, enforce};
use natamux_core::security::{Egress, NetworkSettings};

use crate::check::{LATEST_URL, RELEASE_ROOT, parse_version};
use crate::download::save_verified;
use crate::policy::policy;
use crate::updater::ReleaseFeed;

const ASSET_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

pub struct CurlFeed;

impl ReleaseFeed for CurlFeed {
    fn latest_url(&self) -> Result<String, String> {
        let policy = policy();
        if policy.egress == Egress::Allowlist {
            // The release page the latest address redirects to, its host checked first.
            return checked_target(&policy, LATEST_URL, &mut |url| hop_of(&policy, url));
        }
        let output = curl_with(&policy, LATEST_URL, 30, true)?
            .output()
            .map_err(|error| format!("The update check could not start: {error}"))?;
        if !output.status.success() {
            return Err("The latest release could not be reached".into());
        }
        String::from_utf8(output.stdout)
            .map(|url| url.trim().to_string())
            .map_err(|_| "The latest release URL is not UTF-8".into())
    }

    fn small_file(&self, url: &str, limit: usize) -> Result<Vec<u8>, String> {
        let mut child = get(url, 30)?
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| format!("The release download could not start: {error}"))?;
        let mut body = Vec::new();
        let read = child
            .stdout
            .take()
            .ok_or("The release download has no response")?
            .take(limit as u64 + 1)
            .read_to_end(&mut body);
        if read.is_err() || body.len() > limit {
            let _ = child.kill();
            let _ = child.wait();
            return Err("The release response is too large or could not be read".into());
        }
        if !child.wait().map_err(|error| error.to_string())?.success() {
            return Err("The release download failed".into());
        }
        Ok(body)
    }
}

/// Calls `on_bytes` with the bytes read so far after each read, for download progress.
pub(super) struct Counting<R, F> {
    pub inner: R,
    pub total: u64,
    pub on_bytes: F,
}

impl<R: Read, F: FnMut(u64)> Read for Counting<R, F> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let size = self.inner.read(buf)?;
        self.total = self.total.saturating_add(size as u64);
        (self.on_bytes)(self.total);
        Ok(size)
    }
}

pub fn download_asset(
    url: &str,
    target: &Path,
    sha256: &str,
    on_bytes: impl FnMut(u64),
) -> Result<u64, String> {
    let mut child = get(url, 30 * 60)?
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("The update download could not start: {error}"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or("The update download has no response")?;
    let counted = Counting {
        inner: stdout,
        total: 0,
        on_bytes,
    };
    let saved = save_verified(counted, target, sha256, ASSET_LIMIT);
    if saved.is_err() {
        let _ = child.kill();
    }
    let status = child.wait().map_err(|error| error.to_string())?;
    let size = saved.map_err(|error| error.to_string())?;
    if !status.success() {
        let _ = std::fs::remove_file(target);
        return Err("The update download failed".into());
    }
    Ok(size)
}

pub(super) fn release_url(url: &str) -> bool {
    let Some(path) = url
        .strip_prefix(RELEASE_ROOT)
        .and_then(|rest| rest.strip_prefix('/'))
    else {
        return false;
    };
    let parts: Vec<&str> = path.split('/').collect();
    let valid_name = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
            && part != "."
            && part != ".."
    };
    match parts.as_slice() {
        ["releases", "latest"] => true,
        ["releases", "download", tag, file] => parse_version(tag).is_some() && valid_name(file),
        _ => false,
    }
}

pub(super) fn host_of(url: &str) -> Option<String> {
    let rest = url.strip_prefix("https://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    if authority.contains('\\') {
        return None;
    }
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

/// A request to a GitHub release address, as the settings allow it.
pub(super) fn curl_with(
    policy: &NetworkSettings,
    url: &str,
    timeout_seconds: u32,
    head: bool,
) -> Result<Command, String> {
    if crate::offline::offline() {
        return Err(crate::offline::UPDATES_OFF.into());
    }
    let host = host_of(url).unwrap_or_default();
    enforce(policy, &Target::Update { host: &host })?;
    if !release_url(url) {
        return Err("The update URL is not a GitHub release address".into());
    }
    Ok(build(policy, url, timeout_seconds, head))
}

/// The request to where a release address redirected, under the allowlist: https only and a
/// listed host.
pub(super) fn curl_target(
    policy: &NetworkSettings,
    target: &str,
    timeout_seconds: u32,
) -> Result<Command, String> {
    let host = host_of(target).ok_or("egress policy: an update redirect that is not https")?;
    enforce(policy, &Target::Update { host: &host })?;
    Ok(build(policy, target, timeout_seconds, false))
}

/// Under the allowlist curl follows no redirect: the first request says where it goes
/// (`hop` gives its Location, empty for none), and that host is checked before a second request.
pub(super) fn checked_target(
    policy: &NetworkSettings,
    url: &str,
    hop: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<String, String> {
    if policy.egress != Egress::Allowlist {
        return Ok(url.to_string());
    }
    let next = hop(url)?;
    let next = next.trim();
    if next.is_empty() {
        return Ok(url.to_string());
    }
    let host = host_of(next).ok_or("egress policy: an update redirect that is not https")?;
    enforce(policy, &Target::Update { host: &host })?;
    Ok(next.to_string())
}

/// The Location of a release address, with curl following nothing.
fn hop_of(policy: &NetworkSettings, url: &str) -> Result<String, String> {
    let output = curl_with(policy, url, 30, true)?
        .output()
        .map_err(|error| format!("The update check could not start: {error}"))?;
    if !output.status.success() {
        return Err("The release address could not be reached".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "The redirect URL is not UTF-8".into())
}

/// The GET of a release address, through its checked redirect under the allowlist.
pub(super) fn get_command(
    policy: &NetworkSettings,
    url: &str,
    timeout_seconds: u32,
    hop: &mut dyn FnMut(&str) -> Result<String, String>,
) -> Result<Command, String> {
    let first = curl_with(policy, url, timeout_seconds, false)?;
    let target = checked_target(policy, url, hop)?;
    if target == url {
        return Ok(first);
    }
    curl_target(policy, &target, timeout_seconds)
}

fn get(url: &str, timeout_seconds: u32) -> Result<Command, String> {
    let policy = policy();
    get_command(&policy, url, timeout_seconds, &mut |url| {
        hop_of(&policy, url)
    })
}

fn build(policy: &NetworkSettings, url: &str, timeout_seconds: u32, head: bool) -> Command {
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    // First, or curl reads a `.curlrc` that could add a proxy or a config of its own.
    command.arg("--disable");
    let allowlist = policy.egress == Egress::Allowlist;
    if !allowlist {
        command.arg("--location");
    }
    command.args([
        "--fail",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        if allowlist { "0" } else { "10" },
        "--max-time",
    ]);
    command.arg(timeout_seconds.to_string());
    command
        .arg("--user-agent")
        .arg(format!("Natamux/{}", env!("CARGO_PKG_VERSION")));
    if let Some(proxy) = &policy.proxy {
        command.arg("--proxy").arg(proxy);
    }
    if head {
        command.arg("--head");
        command
            .arg("--output")
            .arg(if cfg!(windows) { "NUL" } else { "/dev/null" });
        let wanted = if allowlist {
            "%{redirect_url}"
        } else {
            "%{url_effective}"
        };
        command.arg("--write-out").arg(wanted);
    }
    command.arg("--").arg(url);
    command
}
