use natamux_core::security::{OUTSIDE_CHANGE_WARNING, SecurityState, read_value, view_of};

use super::*;

fn view(file: Value, warning: bool) -> SecurityView {
    let state = SecurityState {
        warning: warning.then(|| OUTSIDE_CHANGE_WARNING.to_string()),
        ..read_value(&file)
    };
    view_of(&state)
}

#[test]
fn personal_has_no_lock_and_no_tip() {
    let personal = SecurityView::default();
    assert_eq!(strict_tip(&personal), None);
    assert_eq!(permission_lock(&personal), None);
    assert_eq!(ci_lock(&personal), None);
}

#[test]
fn strict_locks_the_permission_mode_and_lists_its_rules_with_the_warning() {
    let strict = view(json!({ "strict": true }), true);
    assert!(permission_lock(&strict).is_some());
    assert_eq!(ci_lock(&strict), None, "approved-commit still checks on CI");
    let tip = strict_tip(&strict).unwrap();
    assert!(tip.starts_with("Strict mode is on:"), "{tip}");
    assert!(
        tip.contains("agentAutoMode") && tip.contains(OUTSIDE_CHANGE_WARNING),
        "{tip}"
    );
    let forbid = view(json!({ "strict": true, "ciPush": "forbid" }), false);
    assert!(ci_lock(&forbid).is_some());
}

#[test]
fn the_switch_keeps_the_chosen_keys_and_off_sends_only_strict_false() {
    let chosen = view(
        json!({ "strict": true, "ciPush": "forbid", "agentApi": { "grants": ["tasks.list", "pr.create.own"] } }),
        false,
    );
    assert_eq!(
        switch_body(&chosen, true),
        json!({ "strict": true, "ciPush": "forbid", "agentApi": { "grants": ["tasks.list", "pr.create.own"] } })
    );
    assert_eq!(switch_body(&chosen, false), json!({ "strict": false }));
    assert_eq!(
        switch_body(&SecurityView::default(), true),
        json!({ "strict": true, "ciPush": "approved-commit", "agentApi": { "grants": [] } })
    );
    assert_eq!(
        ci_push_body(&chosen, "approved-snapshot")["ciPush"],
        "approved-snapshot"
    );
}

#[test]
fn lists_every_rule_but_ci_push_with_its_reason_or_why_it_is_off() {
    let personal = view(json!({}), false);
    let rows = rule_rows(&personal);
    assert!(rows.iter().all(|row| row.key != "ciPush"));
    assert!(
        rows.iter()
            .all(|row| row.help.contains("Strict mode is off"))
    );
    assert_eq!(ci_push_index(&personal), 0);
    let strict = view(json!({ "strict": true, "ciPush": "forbid" }), false);
    let rows = rule_rows(&strict);
    let found = |key: &str| rows.iter().find(|row| row.key == key).unwrap().clone();
    let mode = found("agents.permissionMode");
    assert_eq!(mode.label, "Agent permission mode");
    assert_eq!(mode.value, "ask");
    assert_eq!(
        Some(mode.help.as_str()),
        permission_lock(&strict).as_deref()
    );
    let grants = found("agentApi.grants");
    assert_eq!(grants.value, "none");
    assert_eq!(CI_PUSH_CHOICES[ci_push_index(&strict)], "forbid");
}
