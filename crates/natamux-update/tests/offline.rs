//! A test file is its own process, so the switch stays on for all of it.

use natamux_update::offline::{UPDATES_OFF, set_setting};
use natamux_update::transport::{CurlFeed, download_asset};
use natamux_update::updater::ReleaseFeed;

const RELEASE: &str = "https://github.com/lenhatthanh20/natamux/releases/latest";

#[test]
fn no_update_request_is_started_offline() {
    set_setting(true);
    assert_eq!(CurlFeed.latest_url().err().as_deref(), Some(UPDATES_OFF));
    assert_eq!(
        CurlFeed.small_file(RELEASE, 1024).err().as_deref(),
        Some(UPDATES_OFF)
    );
    let target = std::env::temp_dir().join(format!("natamux-offline-{}", std::process::id()));
    let sha = "0".repeat(64);
    let result = download_asset(RELEASE, &target, &sha, |_| {});
    assert_eq!(result.err().as_deref(), Some(UPDATES_OFF));
    assert!(!target.exists());
}
