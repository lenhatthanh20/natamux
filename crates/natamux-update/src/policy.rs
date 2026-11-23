//! The `network` settings for updates: the daemon's `security.json`, read again on each check,
//! so a change applies without a restart. Machine offline mode is `loopback`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use natamux_core::egress::{Target, check};
use natamux_core::security::{Egress, NetworkSettings, Source, network_in};

static DATA_DIR: Mutex<Option<PathBuf>> = Mutex::new(None);

/// The app calls this once at start with the daemon's data folder.
pub fn set_data_dir(dir: &Path) {
    if let Ok(mut set) = DATA_DIR.lock() {
        *set = Some(dir.to_path_buf());
    }
}

/// The settings in `dir`: Strict from the same file decides the defaults.
pub fn policy_in(dir: &Path) -> NetworkSettings {
    let strict = Source::open(dir).current().mode.is_strict();
    network_in(dir, strict)
}

pub fn policy() -> NetworkSettings {
    let dir = DATA_DIR.lock().ok().and_then(|dir| dir.clone());
    let mut settings = dir.map_or_else(|| NetworkSettings::defaults(false), |dir| policy_in(&dir));
    if crate::offline::offline() {
        settings.egress = Egress::Loopback;
    }
    settings
}

/// Why updates are off by the settings now, if they are.
pub fn updates_off() -> Option<String> {
    check(&policy(), &Target::Update { host: "github.com" }).err()
}
