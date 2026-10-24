//! The trusted part of an update check. A feed supplies the network response.

use crate::check::{InstallKind, Release, asset_name, is_newer, release_from_url, sha256_from};
use crate::signature::{MAX_SIGNED_BYTES, verify_sums};

pub trait ReleaseFeed {
    /// The final URL after HEAD /releases/latest redirects.
    fn latest_url(&self) -> Result<String, String>;
    /// A response body with an enforced byte limit.
    fn small_file(&self, url: &str, limit: usize) -> Result<Vec<u8>, String>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CheckOutcome {
    UpToDate(Release),
    Manual(Release),
    Asset {
        release: Release,
        name: &'static str,
        sha256: String,
    },
}

pub fn check_now(
    feed: &impl ReleaseFeed,
    current: &str,
    kind: InstallKind,
    arch: &str,
    public_key: &str,
) -> Result<CheckOutcome, String> {
    let url = feed.latest_url()?;
    let release = release_from_url(&url).ok_or("The latest release is not a stable GitHub tag")?;
    if !is_newer(&release.version, current) {
        return Ok(CheckOutcome::UpToDate(release));
    }
    let sums = small(feed, &format!("{}SHA256SUMS.txt", release.downloads))?;
    let signature = small(feed, &format!("{}SHA256SUMS.txt.minisig", release.downloads))?;
    let signature = std::str::from_utf8(&signature)
        .map_err(|_| "The release signature is not UTF-8")?;
    verify_sums(&sums, signature, public_key, &release.tag, current)?;
    let Some(name) = asset_name(kind, arch) else {
        return Ok(CheckOutcome::Manual(release));
    };
    let sums_text = std::str::from_utf8(&sums)
        .map_err(|_| "SHA256SUMS.txt is not UTF-8")?;
    let sha256 = sha256_from(sums_text, name)
        .ok_or_else(|| format!("SHA256SUMS.txt has no hash for {name}"))?;
    Ok(CheckOutcome::Asset { release, name, sha256 })
}

fn small(feed: &impl ReleaseFeed, url: &str) -> Result<Vec<u8>, String> {
    let body = feed.small_file(url, MAX_SIGNED_BYTES)?;
    if body.len() > MAX_SIGNED_BYTES {
        Err("The signed files are too large".into())
    } else {
        Ok(body)
    }
}
