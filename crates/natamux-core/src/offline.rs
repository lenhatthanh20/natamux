//! The offline switch: with it on, Natamux's own code and the programs it starts by itself make
//! no connection beyond loopback.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

pub const OFFLINE_ENV: &str = "NATAMUX_OFFLINE";

// Lets a test process turn the switch on without `set_var`, which is unsafe in edition 2024.
static FORCED: AtomicBool = AtomicBool::new(false);

/// On only for the exact value `1`.
pub fn is_on(value: Option<&str>) -> bool {
    value == Some("1")
}

/// Read once per process.
pub fn offline() -> bool {
    static FROM_ENV: OnceLock<bool> = OnceLock::new();
    let from_env = *FROM_ENV.get_or_init(|| is_on(std::env::var(OFFLINE_ENV).ok().as_deref()));
    from_env || FORCED.load(Ordering::Relaxed)
}

/// Turns the switch on for the rest of the process. For tests of other crates.
pub fn force(on: bool) {
    FORCED.store(on, Ordering::Relaxed);
}

#[cfg(test)]
mod tests {
    use super::is_on;

    #[test]
    fn only_the_exact_value_one_turns_it_on() {
        assert!(is_on(Some("1")));
        assert!(!is_on(None));
        assert!(!is_on(Some("")));
        assert!(!is_on(Some("0")));
        assert!(!is_on(Some("true")));
        assert!(!is_on(Some(" 1")));
        assert!(!is_on(Some("11")));
    }
}
