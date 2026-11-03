//! The user's custom agents (Settings > Agents), for the whole process. The daemon sets them at
//! start and on each save, so every check of an agent id (start, resume, signals, workflow) sees
//! the same list without passing it through each service.
//!
//! shortcut: one list per process, which fits one daemon per process; pass a handle instead if a
//! process ever runs two daemons with other lists. Tests that change it must run in one test.

use std::sync::{PoisonError, RwLock};

use natamux_proto::agents::custom::CustomAgent;
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;

static AGENTS: RwLock<Vec<CustomAgent>> = RwLock::new(Vec::new());

pub fn set_custom_agents(agents: Vec<CustomAgent>) {
    *AGENTS.write().unwrap_or_else(PoisonError::into_inner) = agents;
}

pub fn custom_agents() -> Vec<CustomAgent> {
    AGENTS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

pub fn custom_agent(id: &str) -> Option<CustomAgent> {
    AGENTS
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|agent| agent.id == id)
        .cloned()
}

/// The agent whose launch line, hooks, trust and resume `id` uses: a custom agent's base, else
/// the id itself. A custom agent on base "other" stays itself, so it takes the unknown-agent path.
pub fn agent_kind(id: &str) -> String {
    match custom_agent(id) {
        Some(agent) if LAUNCHABLE_AGENTS.contains(&agent.base.as_str()) => agent.base,
        _ => id.to_string(),
    }
}

/// A built-in agent, or a custom one on a built-in base: it has hooks and can be resumed.
pub fn has_adapter(id: &str) -> bool {
    LAUNCHABLE_AGENTS.contains(&agent_kind(id).as_str())
}

/// Every id an agent can be started with: the built-ins, then the custom ones.
pub fn known_agents() -> Vec<String> {
    let mut ids: Vec<String> = LAUNCHABLE_AGENTS.iter().map(|id| id.to_string()).collect();
    ids.extend(custom_agents().into_iter().map(|agent| agent.id));
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, base: &str, command: &str) -> CustomAgent {
        CustomAgent {
            id: id.into(),
            base: base.into(),
            command: command.into(),
        }
    }

    // The only test of this crate that sets the list (see the module note).
    #[test]
    fn resolves_a_custom_agent_to_its_base() {
        set_custom_agents(vec![
            agent("claude-qwen", "claude-code", "cq"),
            agent("my-agent", "other", "my-agent"),
        ]);
        assert_eq!(agent_kind("claude-qwen"), "claude-code");
        assert_eq!(agent_kind("my-agent"), "my-agent");
        assert_eq!(agent_kind("codex"), "codex");
        assert_eq!(agent_kind("aider"), "aider");
        assert!(has_adapter("claude-qwen") && has_adapter("cursor"));
        assert!(!has_adapter("my-agent") && !has_adapter("aider"));
        assert!(crate::launch::is_launchable("my-agent"));
        assert!(!crate::launch::is_launchable("aider"));
        assert_eq!(known_agents().last().map(String::as_str), Some("my-agent"));
        assert_eq!(custom_agent("claude-qwen").unwrap().command, "cq");
    }
}
