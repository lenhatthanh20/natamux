//! The rules of the app's Strict mode views, with no gpui: the status bar lock, the controls it
//! turns off with a reason, and the bodies of a save (`PUT /security`).

use natamux_proto::projects::security::SecurityView;
use serde_json::{Value, json};

/// The `ciPush` choices of Strict mode, loosest first.
pub const CI_PUSH_CHOICES: [&str; 3] = ["approved-commit", "approved-snapshot", "forbid"];

/// The tooltip of the status bar's "Strict" item: each rule that holds now. None in Personal.
pub fn strict_tip(view: &SecurityView) -> Option<String> {
    if !view.strict {
        return None;
    }
    let mut tip = String::from("Strict mode is on:");
    for (key, state) in &view.keys {
        if let Some(reason) = state.reason.as_deref().filter(|_| state.blocked) {
            tip.push_str(&format!("\n\u{2022} {key}: {reason}"));
        }
    }
    if let Some(warning) = &view.warning {
        tip.push_str(&format!("\n\n{warning}"));
    }
    Some(tip)
}

/// Why the permission mode control is off: Strict holds agents to ask.
pub fn permission_lock(view: &SecurityView) -> Option<String> {
    view.blocked("agents.permissionMode").map(str::to_string)
}

/// Why the CI toggle of a stage is off: Strict with `ciPush: forbid`.
pub fn ci_lock(view: &SecurityView) -> Option<String> {
    (view.value("ciPush") == "forbid")
        .then(|| view.blocked("ciPush").map(str::to_string))
        .flatten()
}

/// The labels of the `ciPush` choices, in the order of `CI_PUSH_CHOICES`.
pub const CI_PUSH_LABELS: [&str; 3] = ["Last commit", "Approved tree", "Never"];

/// The `ciPush` choice shown now; Personal shows the first.
pub fn ci_push_index(view: &SecurityView) -> usize {
    let value = view.value("ciPush");
    CI_PUSH_CHOICES
        .iter()
        .position(|choice| *choice == value)
        .unwrap_or(0)
}

/// One rule of the Security page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuleRow {
    pub key: String,
    pub label: &'static str,
    pub value: String,
    /// What the rule does while Strict holds it.
    pub help: String,
}

fn rule_label(key: &str) -> &'static str {
    match key {
        "agentAutoMode" => "Agent auto mode",
        "terminalInputTrace" => "Terminal input trace",
        "repoTrust.scope" => "Repo trust",
        "repoTrust.bypassHookTrust" => "Codex hook trust bypass",
        "agents.permissionMode" => "Agent permission mode",
        "agents.requireAskGuarantee" => "Ask guarantee",
        "agentApi.mode" => "Agent API",
        "agentApi.requireHumanForPush" => "Push needs a person",
        "agentApi.grants" => "Agent grants",
        _ => "Other rule",
    }
}

/// The rules under the switch: every key but `ciPush`, which has its own control.
pub fn rule_rows(view: &SecurityView) -> Vec<RuleRow> {
    view.keys
        .iter()
        .filter(|(key, _)| key.as_str() != "ciPush")
        .map(|(key, state)| RuleRow {
            key: key.clone(),
            label: rule_label(key),
            value: if state.value.is_empty() {
                "none".to_owned()
            } else {
                state.value.clone()
            },
            help: match state.reason.as_deref().filter(|_| state.blocked) {
                Some(reason) => reason.to_owned(),
                None if view.strict => "Set by the grants you list.".to_owned(),
                None => "Not held: Strict mode is off.".to_owned(),
            },
        })
        .collect()
}

/// The grants listed now, as the save sends them back.
fn grants(view: &SecurityView) -> Vec<String> {
    view.value("agentApi.grants")
        .split(',')
        .map(str::trim)
        .filter(|grant| !grant.is_empty())
        .map(str::to_string)
        .collect()
}

fn strict_body(ci_push: &str, view: &SecurityView) -> Value {
    json!({ "strict": true, "ciPush": ci_push, "agentApi": { "grants": grants(view) } })
}

/// The Strict switch. Turning it on keeps the keys that were chosen before.
pub fn switch_body(view: &SecurityView, on: bool) -> Value {
    if !on {
        return json!({ "strict": false });
    }
    let ci_push = view.value("ciPush");
    let ci_push = if CI_PUSH_CHOICES.contains(&ci_push) {
        ci_push
    } else {
        CI_PUSH_CHOICES[0]
    };
    strict_body(ci_push, view)
}

/// A pick of the `ciPush` control (Strict only).
pub fn ci_push_body(view: &SecurityView, ci_push: &str) -> Value {
    strict_body(ci_push, view)
}

#[cfg(test)]
#[path = "security_rules_tests.rs"]
mod tests;
