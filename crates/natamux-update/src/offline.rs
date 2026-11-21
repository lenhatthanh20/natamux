//! The offline switch of the app: the `NATAMUX_OFFLINE` variable or the `offline` setting.
//! Same rule as `natamux_core::offline`; this crate does not depend on that one.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

pub const OFFLINE_ENV: &str = "NATAMUX_OFFLINE";

static SETTING: AtomicBool = AtomicBool::new(false);

/// On only for the exact value `1`.
pub fn is_on(value: Option<&str>) -> bool {
    value == Some("1")
}

/// The variable is read once per process.
pub fn offline() -> bool {
    static FROM_ENV: OnceLock<bool> = OnceLock::new();
    let from_env = *FROM_ENV.get_or_init(|| is_on(std::env::var(OFFLINE_ENV).ok().as_deref()));
    from_env || SETTING.load(Ordering::Relaxed)
}

/// The app calls this once at start with the `offline` key of its settings file.
pub fn set_setting(on: bool) {
    SETTING.store(on, Ordering::Relaxed);
}

pub const UPDATES_OFF: &str = "Updates are off in offline mode";

#[cfg(test)]
mod tests {
    use super::is_on;

    #[test]
    fn only_the_exact_value_one_turns_it_on() {
        assert!(is_on(Some("1")));
        assert!(!is_on(None));
        assert!(!is_on(Some("")));
        assert!(!is_on(Some("true")));
        assert!(!is_on(Some("0")));
    }
}
