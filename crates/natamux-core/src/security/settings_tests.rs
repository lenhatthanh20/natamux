use serde_json::json;

use super::*;

fn strict_of(state: &SecurityState) -> &StrictSettings {
    state.mode.strict().unwrap()
}

#[test]
fn without_strict_true_the_file_is_personal() {
    assert_eq!(read_text("{}").mode, SecurityMode::Personal);
    assert_eq!(
        read_text(r#"{"strict": false, "ciPush": "forbid"}"#).mode,
        SecurityMode::Personal
    );
    assert_eq!(
        read_text(r#"{"strict": "yes"}"#).mode,
        SecurityMode::Personal
    );
}

#[test]
fn strict_true_gives_the_defaults() {
    let state = read_text(r#"{"strict": true}"#);
    assert_eq!(state.mode, SecurityMode::Strict(StrictSettings::default()));
    let strict = strict_of(&state);
    assert_eq!(strict.ci_push, CiPush::ApprovedCommit);
    assert!(strict.require_ask_guarantee);
    assert!(strict.agent_api.scoped && strict.agent_api.require_human_for_push);
    assert!(state.ignored.is_empty() && state.warning.is_none());
}

#[test]
fn a_key_looser_than_the_strict_default_is_ignored_and_named() {
    let state = read_value(&json!({
        "strict": true,
        "ciPush": "snapshot",
        "agentAutoMode": "allow",
        "terminalInputTrace": "on",
        "repoTrust": { "scope": "repo", "bypassHookTrust": true },
        "agents": { "permissionMode": "auto", "requireAskGuarantee": false },
        "agentApi": { "mode": "off", "requireHumanForPush": false },
        "someLaterKey": { "x": 1 },
    }));
    assert_eq!(state.mode, SecurityMode::Strict(StrictSettings::default()));
    let mut ignored = state.ignored.clone();
    ignored.sort();
    assert_eq!(
        ignored,
        [
            "agentApi.mode",
            "agentApi.requireHumanForPush",
            "agentAutoMode",
            "agents.permissionMode",
            "agents.requireAskGuarantee",
            "ciPush",
            "repoTrust.bypassHookTrust",
            "repoTrust.scope",
            "terminalInputTrace",
        ]
    );
}

#[test]
fn stricter_keys_and_grants_are_kept() {
    let state = read_value(&json!({
        "strict": true,
        "ciPush": "forbid",
        "agentAutoMode": "forbid",
        "agentApi": { "grants": ["pr.create.own"], "requireHumanForPush": true },
    }));
    let strict = strict_of(&state);
    assert_eq!(strict.ci_push, CiPush::Forbid);
    assert_eq!(strict.agent_api.grants, ["pr.create.own"]);
    assert!(state.ignored.is_empty());
}

#[test]
fn bad_json_fails_closed_with_a_warning() {
    for text in ["{", "", "[1]", "null"] {
        let state = read_text(text);
        assert_eq!(
            state.mode,
            SecurityMode::Strict(StrictSettings::default()),
            "{text}"
        );
        assert_eq!(state.warning.as_deref(), Some(BAD_FILE_WARNING), "{text}");
    }
}

#[test]
fn the_written_text_reads_back_as_the_same_mode() {
    let strict = SecurityMode::Strict(StrictSettings {
        ci_push: CiPush::ApprovedSnapshot,
        agent_api: AgentApi {
            grants: vec!["tasks.list".into()],
            ..AgentApi::default()
        },
        ..StrictSettings::default()
    });
    for mode in [SecurityMode::Personal, strict] {
        assert_eq!(read_text(&to_text(&mode)).mode, mode);
    }
}
