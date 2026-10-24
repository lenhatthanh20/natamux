//! The group 'the adapter list'.

use natamux_agents::hooks::{find_adapter, known_agent_ids};

#[test]
fn knows_claude_code() {
    assert!(known_agent_ids().contains(&"claude-code"));
    assert_eq!(
        find_adapter("claude-code").map(|adapter| adapter.id()),
        Some("claude-code")
    );
}

#[test]
fn knows_no_adapter_for_another_name() {
    assert!(find_adapter("nope").is_none());
}
