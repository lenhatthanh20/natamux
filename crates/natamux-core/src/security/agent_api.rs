//! What the agent token of a task may call in Strict mode: the `agentApi` key.

/// `agentApi` of Strict mode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentApi {
    /// `mode: scoped`: an agent gets only the minimum grants plus `grants`. Off: the Personal
    /// rules (own task and direct subtasks).
    pub scoped: bool,
    /// The opt-in grants by name, such as `git.push.own`.
    pub grants: Vec<String>,
    /// `git.push.own` is never granted: a person pushes.
    pub require_human_for_push: bool,
}

impl Default for AgentApi {
    fn default() -> Self {
        AgentApi {
            scoped: true,
            grants: Vec::new(),
            require_human_for_push: true,
        }
    }
}
