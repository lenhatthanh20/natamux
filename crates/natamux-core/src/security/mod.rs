//! The security mode: Personal (today's workflow) or Strict. Every Strict rule reads `mode()`.
//! Until the user setting exists it is always Personal; tests turn Strict on with `force`.

use std::cell::RefCell;

mod ci_push;

pub use ci_push::{CI_FORBID_REASON, CiMode, CiPush, ci_mode};

/// The Strict keys the Strict rules read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrictSettings {
    /// Only agents that Natamux can hold to asking before each action may start.
    pub require_ask_guarantee: bool,
    /// What a CI check may push (`ciPush`).
    pub ci_push: CiPush,
}

impl Default for StrictSettings {
    fn default() -> Self {
        StrictSettings {
            require_ask_guarantee: true,
            ci_push: CiPush::default(),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum SecurityMode {
    #[default]
    Personal,
    Strict(StrictSettings),
}

impl SecurityMode {
    pub fn is_strict(&self) -> bool {
        matches!(self, SecurityMode::Strict(_))
    }

    pub fn strict(&self) -> Option<&StrictSettings> {
        match self {
            SecurityMode::Strict(settings) => Some(settings),
            SecurityMode::Personal => None,
        }
    }
}

thread_local! {
    // Per thread, so a test that forces Strict never changes another test running beside it.
    static FORCED: RefCell<Option<SecurityMode>> = const { RefCell::new(None) };
}

/// The mode for this check.
pub fn mode() -> SecurityMode {
    FORCED
        .with(|forced| forced.borrow().clone())
        .unwrap_or_default()
}

/// The mode a test forced on this thread, so work this thread hands to another thread can force
/// the same mode there.
pub fn forced() -> Option<SecurityMode> {
    FORCED.with(|forced| forced.borrow().clone())
}

/// Test hook: `mode()` gives `forced` on this thread until the guard is dropped.
pub fn force(forced: SecurityMode) -> Forced {
    let previous = FORCED.with(|cell| cell.replace(Some(forced)));
    Forced { previous }
}

#[must_use = "the mode goes back when the guard is dropped"]
pub struct Forced {
    previous: Option<SecurityMode>,
}

impl Drop for Forced {
    fn drop(&mut self) {
        let previous = self.previous.take();
        FORCED.with(|cell| cell.replace(previous));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn is_personal_unless_a_test_forces_strict_and_goes_back_after() {
        assert_eq!(mode(), SecurityMode::Personal);
        {
            let _strict = force(SecurityMode::Strict(StrictSettings::default()));
            assert!(mode().is_strict());
            assert_eq!(
                mode().strict().map(|it| it.require_ask_guarantee),
                Some(true)
            );
        }
        assert_eq!(mode(), SecurityMode::Personal);
    }
}
