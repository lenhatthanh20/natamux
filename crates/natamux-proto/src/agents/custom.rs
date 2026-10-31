//! Custom agents (`GET`/`PUT /custom-agents`): an agent the user adds in Settings > Agents. It runs
//! as a built-in agent (`base`) with another command, such as Claude Code started by a shell
//! function that points it at another model, or as any other CLI (`OTHER_BASE`).

use serde::{Deserialize, Serialize};

/// The base of a custom agent that is no built-in one: Natamux sees it by the generic signals only.
pub const OTHER_BASE: &str = "other";

/// At most this many custom agents.
pub const MAX_CUSTOM_AGENTS: usize = 20;

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomAgent {
    pub id: String,
    /// A `LAUNCHABLE_AGENTS` id, or `OTHER_BASE`.
    pub base: String,
    /// The program in place of the base's, as the terminal's shell runs it.
    pub command: String,
}

/// The body of `GET /custom-agents`, of `PUT /custom-agents` and of its answer.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
pub struct CustomAgentList {
    #[serde(default)]
    pub agents: Vec<CustomAgent>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_and_writes_the_list() {
        let text = r#"{"agents":[{"id":"claude-qwen","base":"claude-code","command":"cq"}]}"#;
        let list: CustomAgentList = serde_json::from_str(text).unwrap();
        assert_eq!(list.agents[0].command, "cq");
        assert_eq!(serde_json::to_string(&list).unwrap(), text);
        let empty: CustomAgentList = serde_json::from_str("{}").unwrap();
        assert!(empty.agents.is_empty());
    }
}
