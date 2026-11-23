//! The stricter of two modes, a repo's tightening, and what each key blocks now.

use std::path::Path;

use serde_json::Value;

use super::settings::{FIXED_KEYS, read_value};
use super::{AUTO_FORBID_REASON, AgentApi, CI_FORBID_REASON, SecurityMode, StrictSettings};

/// What a key does to the user's work now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    Allowed,
    Blocked { key: &'static str, reason: String },
}

/// The stricter of two modes, key by key. Strict wins over Personal; fewer grants win.
pub fn tighten(a: &SecurityMode, b: &SecurityMode) -> SecurityMode {
    let (a, b) = match (a.strict(), b.strict()) {
        (None, _) => return b.clone(),
        (_, None) => return a.clone(),
        (Some(a), Some(b)) => (a, b),
    };
    let ci_push = if a.ci_push.rank() >= b.ci_push.rank() {
        a.ci_push
    } else {
        b.ci_push
    };
    let grants = a
        .agent_api
        .grants
        .iter()
        .filter(|grant| b.agent_api.grants.contains(grant))
        .cloned()
        .collect();
    SecurityMode::Strict(StrictSettings {
        require_ask_guarantee: a.require_ask_guarantee || b.require_ask_guarantee,
        ci_push,
        agent_api: AgentApi {
            scoped: a.agent_api.scoped || b.agent_api.scoped,
            grants,
            require_human_for_push: a.agent_api.require_human_for_push
                || b.agent_api.require_human_for_push,
        },
    })
}

/// `mode` with the `security` key of a repo's `.natamux.json` on top. Repo content can only
/// tighten: it may turn Strict on or close a key, never open one.
pub fn tighten_with_repo(mode: &SecurityMode, repo: &Value) -> SecurityMode {
    let Some(Value::Object(keys)) = repo.get("security") else {
        return mode.clone();
    };
    let mut keys = keys.clone();
    if mode.is_strict() {
        // Under Strict the repo's keys count even when it does not say `strict` itself.
        keys.insert("strict".into(), Value::Bool(true));
    }
    tighten(mode, &read_value(&Value::Object(keys)).mode)
}

/// The mode for work in `folder` (a worktree or a project): the user's mode tightened by the
/// folder's `.natamux.json`. A missing or broken file changes nothing.
pub fn mode_in(folder: &Path) -> SecurityMode {
    let mode = super::mode();
    let Ok(text) = std::fs::read_to_string(folder.join(".natamux.json")) else {
        return mode;
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(repo) => tighten_with_repo(&mode, &repo),
        Err(_) => mode,
    }
}

/// One key of the mode: its value and what it blocks, for `GET /security`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeyState {
    pub key: &'static str,
    pub value: String,
    pub decision: Decision,
}

fn blocked(key: &'static str, reason: &str) -> Decision {
    Decision::Blocked {
        key,
        reason: reason.to_string(),
    }
}

fn fixed_reason(key: &str) -> &'static str {
    match key {
        "agentAutoMode" => AUTO_FORBID_REASON,
        "terminalInputTrace" => "Typed terminal input is never traced to a log.",
        "repoTrust.scope" => "Agent config is approved per worktree and checked before each start.",
        "repoTrust.bypassHookTrust" => "Codex hook trust is bypassed only for an approved digest.",
        "agents.permissionMode" => "Agents start in the ask permission mode.",
        "agents.requireAskGuarantee" => "Only agents that ask before each action may start.",
        "agentApi.mode" => "An agent token reaches only its own task, with the listed grants.",
        "data.auditLog" => "Blocked actions and security changes are written to logs/audit.log.",
        _ => "An agent never pushes; a person does.",
    }
}

fn ci_reason(ci_push: super::CiPush) -> &'static str {
    match ci_push {
        super::CiPush::ApprovedCommit => "CI gets only the last commit (HEAD).",
        super::CiPush::ApprovedSnapshot => "A person approves the exact tree of each CI push.",
        super::CiPush::Forbid => CI_FORBID_REASON,
    }
}

/// Every key with its value now. In Personal mode nothing is blocked.
pub fn key_states(mode: &SecurityMode) -> Vec<KeyState> {
    let strict = mode.strict();
    let decide = |key: &'static str, reason: &str| match strict {
        Some(_) => blocked(key, reason),
        None => Decision::Allowed,
    };
    let mut keys = vec![KeyState {
        key: "ciPush",
        value: strict.map_or("snapshot", |it| it.ci_push.as_str()).into(),
        decision: decide("ciPush", strict.map_or("", |it| ci_reason(it.ci_push))),
    }];
    for (key, value) in FIXED_KEYS {
        keys.push(KeyState {
            key,
            value: (if strict.is_some() { value } else { "not set" }).into(),
            decision: decide(key, fixed_reason(key)),
        });
    }
    let grants = strict.map_or(String::new(), |it| it.agent_api.grants.join(", "));
    keys.push(KeyState {
        key: "agentApi.grants",
        value: grants,
        decision: Decision::Allowed,
    });
    keys
}

#[cfg(test)]
#[path = "decide_tests.rs"]
mod tests;
