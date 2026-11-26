//! The security mode: Personal (today's workflow) or Strict. Every Strict rule reads `mode()`:
//! a mode a test forced on this thread, else the user's `security.json` once the daemon
//! installed it, else Personal.

use std::cell::RefCell;
use std::sync::{Arc, Mutex, PoisonError};

mod agent_api;
mod agent_mode;
mod ci_push;
mod data;
mod decide;
mod git_harden;
mod isolation;
mod network;
mod settings;
mod source;
mod view;

pub use agent_api::AgentApi;
pub use agent_mode::{AUTO_FORBID_REASON, INPUT_TRACE_BLOCKED, auto_allowed, input_trace_allowed};
pub use ci_push::{CI_FORBID_REASON, CiMode, CiPush, ci_mode};
pub use data::{DEFAULT_LOG_MAX_BYTES, DataSettings, data_in, data_of};
pub use decide::{Decision, KeyState, key_states, mode_in, tighten, tighten_with_repo};
pub use git_harden::{
    git_pins, harden_git, harden_git_at, harden_git_in, hardening_config, install_git_hardening,
    pin_git_dirs, pinned_git_dirs, unpin_git_dirs,
};
pub use isolation::{IsolationSettings, isolation_of, isolation_value, load_isolation};
pub use network::{
    Egress, ForcedNetwork, GH_ACTIONS, GIT_ACTIONS, GhRule, GitRule, NetworkSettings, UPDATE_HOSTS,
    force_network, network, network_for, network_in, network_of, tighten_network,
};
pub use settings::{BAD_FILE_WARNING, FIXED_KEYS, SecurityState, read_text, read_value, to_text};
pub use source::{
    FILE_NAME, MACHINE_STRICT, MACHINE_STRICT_WARNING, OUTSIDE_CHANGE_WARNING, Source,
    WRITTEN_NAME, evaluate, with_machine,
};
pub use view::view_of;

/// The Strict keys the Strict rules read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StrictSettings {
    /// Only agents that Natamux can hold to asking before each action may start.
    pub require_ask_guarantee: bool,
    /// What a CI check may push (`ciPush`).
    pub ci_push: CiPush,
    /// What the agent token of a task may call (`agentApi`).
    pub agent_api: AgentApi,
}

impl Default for StrictSettings {
    fn default() -> Self {
        StrictSettings {
            require_ask_guarantee: true,
            ci_push: CiPush::default(),
            agent_api: AgentApi::default(),
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

// Process-wide: the daemon binary installs its data folder's settings before it starts.
static INSTALLED: Mutex<Option<Arc<Source>>> = Mutex::new(None);

/// Makes `source` the settings `mode()` reads in this process.
pub fn install(source: Arc<Source>) {
    *INSTALLED.lock().unwrap_or_else(PoisonError::into_inner) = Some(source);
}

/// The settings `install` set, if any.
pub fn installed() -> Option<Arc<Source>> {
    INSTALLED
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// The mode for this check. Read again on each decision, so a change applies to the next one.
pub fn mode() -> SecurityMode {
    if let Some(forced) = forced() {
        return forced;
    }
    installed().map_or_else(SecurityMode::default, |source| source.current().mode)
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
