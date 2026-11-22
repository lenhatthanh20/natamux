//! Starting an agent by hand (`POST /tasks/:id/agent`, `packages/daemon/src/http/routes/agents.ts`).

use serde::Serialize;

/// `LAUNCHABLE_AGENTS` in `packages/daemon/src/agents/agent-launch.ts`: the agents the daemon can
/// start. No id has a `-`: it is part of the variables `NATAMUX_<ID>_COMMAND` and `_ARGS`.
pub const LAUNCHABLE_AGENTS: [&str; 16] = [
    "claude-code",
    "codex",
    "gemini",
    "copilot",
    "cursor",
    "opencode",
    "qwen",
    "droid",
    "amp",
    "kimi",
    "goose",
    "aider",
    "crush",
    "cline",
    "kiro",
    "muse",
];

/// The built-in agents with hooks: they report each state and their session. The others are
/// followed by their terminal only. `natamux_agents::hooks` has an adapter for each.
pub const HOOK_AGENTS: [&str; 8] = [
    "claude-code",
    "codex",
    "gemini",
    "copilot",
    "cursor",
    "opencode",
    "qwen",
    "droid",
];

/// The command each agent is started with when no other is configured. The app shows it too.
pub const DEFAULT_PROGRAMS: [(&str, &str); 16] = [
    ("claude-code", "claude"),
    ("codex", "codex"),
    ("gemini", "gemini"),
    ("copilot", "copilot"),
    ("cursor", "cursor-agent"),
    ("opencode", "opencode"),
    ("qwen", "qwen"),
    ("droid", "droid"),
    ("amp", "amp"),
    ("kimi", "kimi"),
    ("goose", "goose"),
    ("aider", "aider"),
    ("crush", "crush"),
    ("cline", "cline"),
    ("kiro", "kiro-cli"),
    ("muse", "muse"),
];

/// The name each agent shows to the user, as the app's agent list shows it.
pub const AGENT_LABELS: [(&str, &str); 16] = [
    ("claude-code", "Claude Code"),
    ("codex", "Codex"),
    ("gemini", "Gemini CLI"),
    ("copilot", "GitHub Copilot"),
    ("cursor", "Cursor"),
    ("opencode", "OpenCode"),
    ("qwen", "Qwen Code"),
    ("droid", "Factory Droid"),
    ("amp", "Amp"),
    ("kimi", "Kimi CLI"),
    ("goose", "Goose"),
    ("aider", "Aider"),
    ("crush", "Crush"),
    ("cline", "Cline CLI"),
    ("kiro", "Kiro CLI"),
    ("muse", "Muse Code (Meta)"),
];

/// The label of an agent; any other id (a custom agent) as it is.
pub fn agent_label(id: &str) -> &str {
    AGENT_LABELS
        .iter()
        .find(|(name, _)| *name == id)
        .map_or(id, |(_, label)| label)
}

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
    fn has_a_program_for_each_agent_and_no_dash_in_an_id_but_claude_code() {
        let names: Vec<&str> = DEFAULT_PROGRAMS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, LAUNCHABLE_AGENTS);
        for agent in LAUNCHABLE_AGENTS.iter().skip(1) {
            assert!(!agent.contains('-'), "{agent}");
        }
    }

    #[test]
    fn has_a_label_for_each_agent_and_keeps_an_unknown_id() {
        let names: Vec<&str> = AGENT_LABELS.iter().map(|(name, _)| *name).collect();
        assert_eq!(names, LAUNCHABLE_AGENTS);
        assert_eq!(agent_label("claude-code"), "Claude Code");
        assert_eq!(agent_label("my-agent"), "my-agent");
    }

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
