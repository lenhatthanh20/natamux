//! The `isolation` key of `security.json`: its own opt-in, not part of the Strict defaults (Strict
//! on macOS would otherwise block every agent). Read once when the daemon starts.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};

use super::source::FILE_NAME;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct IsolationSettings {
    /// `mode: required`: every task shell and command runs isolated, or does not run.
    pub required: bool,
    pub extra_read_only: Vec<PathBuf>,
    pub extra_read_write: Vec<PathBuf>,
    /// More variable names an isolated shell keeps.
    pub env_allow: Vec<String>,
}

fn paths(value: Option<&Value>) -> Vec<PathBuf> {
    strings(value)
        .into_iter()
        .map(PathBuf::from)
        // A relative path would depend on where the daemon started.
        .filter(|path| path.is_absolute())
        .collect()
}

fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|list| {
            list.iter()
                .filter_map(Value::as_str)
                .filter(|text| !text.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// The `isolation` object of a `security.json` value. A mode other than `off` counts as
/// `required`: a typo must not turn isolation off.
pub fn isolation_of(value: &Value) -> IsolationSettings {
    let Some(isolation) = value.get("isolation") else {
        return IsolationSettings::default();
    };
    let required = match isolation.get("mode").and_then(Value::as_str) {
        None | Some("off") => false,
        Some(_) => true,
    };
    IsolationSettings {
        required,
        extra_read_only: paths(isolation.get("extraReadOnly")),
        extra_read_write: paths(isolation.get("extraReadWrite")),
        env_allow: strings(isolation.get("envAllow")),
    }
}

/// The settings as `security.json` stores them.
pub fn isolation_value(settings: &IsolationSettings) -> Value {
    json!({
        "mode": if settings.required { "required" } else { "off" },
        "extraReadOnly": settings.extra_read_only,
        "extraReadWrite": settings.extra_read_write,
        "envAllow": settings.env_allow,
    })
}

/// The isolation settings of the data folder. No file: off. A file that is not JSON: required,
/// so a broken file fails closed, as it does for Strict.
pub fn load_isolation(data_dir: &Path) -> IsolationSettings {
    let Ok(text) = std::fs::read_to_string(data_dir.join(FILE_NAME)) else {
        return IsolationSettings::default();
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(value) => isolation_of(&value),
        Err(_) => IsolationSettings {
            required: true,
            ..IsolationSettings::default()
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_mode_and_the_lists_and_fails_closed_on_an_unknown_mode() {
        assert_eq!(isolation_of(&json!({})), IsolationSettings::default());
        let off = isolation_of(&json!({ "isolation": { "mode": "off" } }));
        assert!(!off.required);
        let set = isolation_of(&json!({ "isolation": {
            "mode": "required",
            "extraReadOnly": ["/opt/tools", "relative/no"],
            "extraReadWrite": ["/srv/cache"],
            "envAllow": ["JAVA_HOME", ""],
        }}));
        assert!(set.required);
        assert_eq!(set.extra_read_only, [PathBuf::from("/opt/tools")]);
        assert_eq!(set.extra_read_write, [PathBuf::from("/srv/cache")]);
        assert_eq!(set.env_allow, ["JAVA_HOME"]);
        assert_eq!(
            isolation_of(&json!({ "isolation": isolation_value(&set) })),
            set
        );
        assert!(isolation_of(&json!({ "isolation": { "mode": "requird" } })).required);
    }

    #[test]
    fn a_missing_file_is_off_and_a_broken_one_is_required() {
        let dir = std::env::temp_dir().join(format!("natamux-iso-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!load_isolation(&dir).required);
        std::fs::write(dir.join(FILE_NAME), "{ not json").unwrap();
        assert!(load_isolation(&dir).required);
        std::fs::write(dir.join(FILE_NAME), r#"{"strict":false}"#).unwrap();
        assert!(!load_isolation(&dir).required);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
