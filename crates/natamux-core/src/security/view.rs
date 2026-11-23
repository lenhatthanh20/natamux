//! The state as the API shows it (`GET /security`): no secrets, so task tokens may read it.

use natamux_proto::projects::security::{SecurityKey, SecurityView};

use super::decide::{Decision, key_states};
use super::settings::SecurityState;

pub fn view_of(state: &SecurityState) -> SecurityView {
    let keys = key_states(&state.mode)
        .into_iter()
        .map(|each| {
            let (blocked, reason) = match each.decision {
                Decision::Allowed => (false, None),
                Decision::Blocked { reason, .. } => (true, Some(reason)),
            };
            let key = SecurityKey {
                value: each.value,
                blocked,
                reason,
            };
            (each.key.to_string(), key)
        })
        .collect();
    SecurityView {
        strict: state.mode.is_strict(),
        keys,
        ignored: state.ignored.clone(),
        warning: state.warning.clone(),
        isolation: None,
        network: None,
    }
}
