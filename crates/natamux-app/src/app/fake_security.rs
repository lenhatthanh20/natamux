//! The fake's security settings: `GET` and `PUT /security`. It starts in the state that
//! `NATAMUX_FAKE_SECURITY` names, for the shots: `strict`, `warning` (Strict with the outside
//! change warning), else Personal.

use natamux_client::ClientError;
use natamux_core::security::{OUTSIDE_CHANGE_WARNING, SecurityState, read_value, view_of};
use natamux_proto::projects::security::SecurityView;
use serde_json::{Value, json};

use super::{Answer, FakeDaemon};

/// The state a fixture name gives.
pub fn fixture(name: &str) -> SecurityState {
    match name {
        "strict" => read_value(&json!({ "strict": true })),
        "warning" => SecurityState {
            warning: Some(OUTSIDE_CHANGE_WARNING.to_string()),
            ..read_value(&json!({ "strict": true }))
        },
        _ => SecurityState::default(),
    }
}

impl FakeDaemon {
    fn security_state(&self) -> SecurityState {
        let mut data = self.data();
        data.security
            .get_or_insert_with(|| {
                fixture(&std::env::var("NATAMUX_FAKE_SECURITY").unwrap_or_default())
            })
            .clone()
    }

    /// The shots: the state of a fixture name, whatever the env says.
    pub fn use_security_fixture(&self, name: &str) {
        self.data().security = Some(fixture(name));
    }

    pub fn security(&self) -> Answer<SecurityView> {
        Ok(view_of(&self.security_state()))
    }

    /// As the daemon: a key looser than Strict is refused, the rest replaces the state.
    pub fn save_security(&self, body: &Value) -> Answer<SecurityView> {
        if !body.get("strict").is_some_and(Value::is_boolean) {
            return Err(refused("strict must be true or false".into()));
        }
        let state = read_value(body);
        if !state.ignored.is_empty() {
            let keys = state.ignored.join(", ");
            return Err(refused(format!(
                "These keys can only be stricter than the Strict default: {keys}"
            )));
        }
        self.data().security = Some(state.clone());
        Ok(view_of(&state))
    }
}

fn refused(message: String) -> ClientError {
    ClientError::Status {
        status: 400,
        message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn starts_personal_and_saves_strict_and_refuses_a_looser_key() {
        let fake = FakeDaemon::default();
        fake.data().security = Some(fixture("personal"));
        assert!(!fake.security().unwrap().strict);
        let saved = fake.save_security(&json!({ "strict": true })).unwrap();
        assert!(saved.strict && saved.blocked("agentAutoMode").is_some());
        assert!(fake.security().unwrap().strict);
        let looser = json!({ "strict": true, "ciPush": "snapshot" });
        assert!(fake.save_security(&looser).is_err());
        assert!(fake.save_security(&json!({})).is_err());
        fake.data().security = Some(fixture("warning"));
        let warned = fake.security().unwrap();
        assert_eq!(warned.warning.as_deref(), Some(OUTSIDE_CHANGE_WARNING));
    }
}
