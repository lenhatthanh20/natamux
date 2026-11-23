//! Reads `security.json`. `"strict": true` turns Strict mode on with its defaults. A key can only
//! make Strict stricter: a looser value is ignored and named in `ignored`. Unknown keys are
//! ignored, so later keys do not break an older build.

use serde_json::Value;

use super::{AgentApi, CiPush, SecurityMode, StrictSettings};

/// The state the file gives.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SecurityState {
    pub mode: SecurityMode,
    /// Keys set looser than the Strict default, by path (`agents.permissionMode`).
    pub ignored: Vec<String>,
    /// Shown to the user: the file is broken, or changed outside Natamux.
    pub warning: Option<String>,
}

pub const BAD_FILE_WARNING: &str = "security.json could not be read, so Strict mode is on with \
     its defaults. Save the Security settings again to fix the file.";

/// The keys Strict mode fixes, with the only value it accepts.
pub const FIXED_KEYS: [(&str, &str); 8] = [
    ("agentAutoMode", "forbid"),
    ("terminalInputTrace", "off"),
    ("repoTrust.scope", "worktree"),
    ("repoTrust.bypassHookTrust", "false"),
    ("agents.permissionMode", "ask"),
    ("agents.requireAskGuarantee", "true"),
    ("agentApi.mode", "scoped"),
    ("agentApi.requireHumanForPush", "true"),
];

fn at<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.').try_fold(value, |node, key| node.get(key))
}

fn text_of(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    }
}

/// The text of the file. Text that is not a JSON object fails closed: the user opted in once,
/// so a broken file must not turn Strict off.
pub fn read_text(text: &str) -> SecurityState {
    match serde_json::from_str::<Value>(text) {
        Ok(value) if value.is_object() => read_value(&value),
        _ => SecurityState {
            mode: SecurityMode::Strict(StrictSettings::default()),
            ignored: Vec::new(),
            warning: Some(BAD_FILE_WARNING.to_string()),
        },
    }
}

/// A JSON object in the shape of `security.json`.
pub fn read_value(value: &Value) -> SecurityState {
    if value.get("strict") != Some(&Value::Bool(true)) {
        return SecurityState::default();
    }
    let mut ignored = Vec::new();
    for (path, wanted) in FIXED_KEYS {
        if let Some(set) = at(value, path)
            && text_of(set) != wanted
        {
            ignored.push(path.to_string());
        }
    }
    let ci_push = match at(value, "ciPush").map(text_of).as_deref() {
        None | Some("approved-commit") => CiPush::ApprovedCommit,
        Some("approved-snapshot") => CiPush::ApprovedSnapshot,
        Some("forbid") => CiPush::Forbid,
        Some(_) => {
            ignored.push("ciPush".into());
            CiPush::ApprovedCommit
        }
    };
    let grants = match at(value, "agentApi.grants") {
        None => Vec::new(),
        Some(Value::Array(list)) => list
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect(),
        Some(_) => {
            ignored.push("agentApi.grants".into());
            Vec::new()
        }
    };
    let settings = StrictSettings {
        ci_push,
        agent_api: AgentApi {
            grants,
            ..AgentApi::default()
        },
        ..StrictSettings::default()
    };
    SecurityState {
        mode: SecurityMode::Strict(settings),
        ignored,
        warning: None,
    }
}

/// The file text for a mode, as `PUT /security` writes it.
pub fn to_text(mode: &SecurityMode) -> String {
    let value = match mode.strict() {
        None => serde_json::json!({ "strict": false }),
        Some(strict) => serde_json::json!({
            "strict": true,
            "ciPush": strict.ci_push.as_str(),
            "agentApi": { "grants": strict.agent_api.grants },
        }),
    };
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

#[cfg(test)]
#[path = "settings_tests.rs"]
mod tests;
