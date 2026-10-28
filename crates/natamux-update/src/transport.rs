//! HTTPS release downloads through the system curl executable.

use std::io::{self, Read};
use std::path::Path;
use std::process::{Command, Stdio};

use crate::check::{LATEST_URL, RELEASE_ROOT, parse_version};
use crate::download::save_verified;
use crate::updater::ReleaseFeed;

const ASSET_LIMIT: u64 = 2 * 1024 * 1024 * 1024;

pub struct CurlFeed;

impl ReleaseFeed for CurlFeed {
    fn latest_url(&self) -> Result<String, String> {
        let output = curl(LATEST_URL, 30, true)?
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
        let mut child = curl(url, 30, false)?
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
    let mut child = curl(url, 30 * 60, false)?
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

pub(super) fn curl(url: &str, timeout_seconds: u32, head: bool) -> Result<Command, String> {
    if !release_url(url) {
        return Err("The update URL is not a GitHub release address".into());
    }
    let mut command = Command::new(if cfg!(windows) { "curl.exe" } else { "curl" });
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    command.args([
        "--location",
        "--fail",
        "--silent",
        "--show-error",
        "--proto",
        "=https",
        "--proto-redir",
        "=https",
        "--max-redirs",
        "10",
        "--max-time",
    ]);
    command.arg(timeout_seconds.to_string());
    command
        .arg("--user-agent")
        .arg(format!("Natamux/{}", env!("CARGO_PKG_VERSION")));
    if head {
        command.arg("--head");
        command
            .arg("--output")
            .arg(if cfg!(windows) { "NUL" } else { "/dev/null" });
        command.arg("--write-out").arg("%{url_effective}");
    }
    command.arg("--").arg(url);
    Ok(command)
}
