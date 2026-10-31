//! Starting an agent by hand (`POST /tasks/:id/agent`, `packages/daemon/src/http/routes/agents.ts`).

use serde::Serialize;

/// `LAUNCHABLE_AGENTS` in `packages/daemon/src/agents/agent-launch.ts`: the agents the daemon can
/// start.
pub const LAUNCHABLE_AGENTS: [&str; 5] = ["claude-code", "codex", "gemini", "copilot", "cursor"];

/// The command each agent is started with when no other is configured. The app shows it too.
pub const DEFAULT_PROGRAMS: [(&str, &str); 5] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("copilot", "copilot"),
    ("cursor", "cursor-agent"),
];

/// `prompt` of a start: at most this many characters.
pub const MAX_START_PROMPT_LENGTH: usize = 20_000;

/// The body of `POST /tasks/:id/agent`. `base` is the branch a workspace is made from when the
/// task has none yet. The answer is `{ task }` (`TaskAnswer`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct StartAgentInput {
    pub agent: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
}

/// The body of `POST /tasks/:id/agent/message`: text for the running agent, trimmed, 1 to
/// `MAX_AGENT_MESSAGE` (in `workspaces/changes.rs`). The answer is `{ task }` (`TaskAnswer`).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AgentMessage {
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_the_message_as_text() {
        let message = AgentMessage { text: "hi".into() };
        assert_eq!(serde_json::to_string(&message).unwrap(), r#"{"text":"hi"}"#);
    }

    #[test]
    fn sends_only_what_is_set() {
        let input = StartAgentInput {
            agent: "codex".into(),
            base: Some("dev".into()),
            ..Default::default()
        };
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"agent":"codex","base":"dev"}"#
        );
    }
}
