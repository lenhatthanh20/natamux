//! The `network` settings for updates: the daemon's `security.json`, read again on each check,
//! so a change applies without a restart. Machine offline mode is `loopback`, and the machine key
//! `"updates": false` turns updates off.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use natamux_core::egress::{Target, check};
use natamux_core::security::{Egress, NetworkSettings, Source, network_in};

pub const UPDATES_BY_ADMIN: &str = "Updates are turned off by your administrator";

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
    if natamux_core::offline::machine_updates_off() {
        settings.updater = false;
    }
    settings
}

/// Why updates are off by the settings now, if they are.
pub fn updates_off() -> Option<String> {
    updates_off_with(natamux_core::offline::machine_updates_off(), &policy())
}

/// Why updates are off, given the machine key `"updates": false` and the settings.
pub fn updates_off_with(machine_off: bool, settings: &NetworkSettings) -> Option<String> {
    if machine_off {
        return Some(UPDATES_BY_ADMIN.into());
    }
    check(settings, &Target::Update { host: "github.com" }).err()
}
