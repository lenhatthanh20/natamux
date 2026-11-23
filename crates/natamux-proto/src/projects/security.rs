//! `GET /security` and `PUT /security`: the security mode, what each key blocks, the keys set
//! looser than Strict (ignored) and the warning when the file changed outside Natamux.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// One key: its value now, and why it blocks when it does.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityKey {
    pub value: String,
    pub blocked: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecurityView {
    pub strict: bool,
    #[serde(default)]
    pub keys: BTreeMap<String, SecurityKey>,
    #[serde(default)]
    pub ignored: Vec<String>,
    #[serde(default)]
    pub warning: Option<String>,
}

impl SecurityView {
    /// The reason of a key while it blocks.
    pub fn blocked(&self, key: &str) -> Option<&str> {
        self.keys
            .get(key)
            .filter(|state| state.blocked)
            .and_then(|state| state.reason.as_deref())
    }

    /// The value of a key, empty when unknown.
    pub fn value(&self, key: &str) -> &str {
        self.keys.get(key).map_or("", |state| state.value.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_body_of_the_route_and_its_blocked_keys() {
        let view: SecurityView = serde_json::from_str(
            r#"{"strict":true,"keys":{"ciPush":{"value":"forbid","blocked":true,"reason":"No CI."},
            "agentApi.grants":{"value":"","blocked":false,"reason":null}},"ignored":[],"warning":null}"#,
        )
        .unwrap();
        assert!(view.strict);
        assert_eq!(view.blocked("ciPush"), Some("No CI."));
        assert_eq!(view.value("ciPush"), "forbid");
        assert_eq!(view.blocked("agentApi.grants"), None);
        assert_eq!(view.blocked("missing"), None);
        let personal: SecurityView = serde_json::from_str(r#"{"strict":false}"#).unwrap();
        assert_eq!(personal, SecurityView::default());
    }
}
