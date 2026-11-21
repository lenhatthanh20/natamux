//! The offline switch of the app: `natamux_core::offline`, or the `offline` key of the user's own
//! settings file.

use std::sync::atomic::{AtomicBool, Ordering};

static SETTING: AtomicBool = AtomicBool::new(false);

pub fn offline() -> bool {
    natamux_core::offline::offline() || SETTING.load(Ordering::Relaxed)
}

/// The app calls this once at start with the `offline` key of its settings file.
pub fn set_setting(on: bool) {
    SETTING.store(on, Ordering::Relaxed);
}

pub const UPDATES_OFF: &str = "Updates are off in offline mode";
