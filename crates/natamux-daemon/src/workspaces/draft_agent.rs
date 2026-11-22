//! Which agent drafts a commit message: the task's agent when it can answer once and is
//! installed, else the first installed agent that can. A user without Claude Code still gets a
//! draft from the agent they have.

use natamux_agents::launch::{LAUNCHABLE_AGENTS, has_one_shot, is_launchable};
use natamux_proto::agents::start::agent_label;

/// The start of the reason when no agent that can draft is installed.
pub const NO_DRAFT_AGENT: &str = "No agent CLI that can draft a message is installed";

#[derive(Debug, PartialEq, Eq)]
pub enum DraftPick {
    /// The agent to run, and why it is not the task's agent (None when it is, or the task has
    /// none).
    Agent { agent: String, note: Option<String> },
    /// No candidate is installed: the reason for the user, with the agents that were looked for.
    Missing(String),
}

/// The agents that may draft, best first: the task's agent kind when it has an answer-only
/// mode, then every such agent in the order of `LAUNCHABLE_AGENTS` (Claude Code first).
pub fn draft_agents(task_kind: Option<&str>) -> Vec<String> {
    let mut agents: Vec<String> = task_kind
        .filter(|kind| has_one_shot(kind))
        .map(str::to_string)
        .into_iter()
        .collect();
    for agent in LAUNCHABLE_AGENTS {
        if has_one_shot(agent) && !agents.iter().any(|known| known == agent) {
            agents.push(agent.to_string());
        }
    }
    agents
}

/// The first installed candidate. `program_of` gives an agent's command line, `is_installed`
/// tells whether that line's program is found.
pub fn pick_draft_agent(
    task_kind: Option<&str>,
    program_of: &dyn Fn(&str) -> String,
    is_installed: &dyn Fn(&str) -> bool,
) -> DraftPick {
    let candidates = draft_agents(task_kind);
    let Some(agent) = candidates
        .iter()
        .find(|agent| is_installed(&program_of(agent)))
    else {
        let labels: Vec<&str> = candidates
            .iter()
            .map(|agent| agent_label(agent.as_str()))
            .collect();
        return DraftPick::Missing(format!("{NO_DRAFT_AGENT}: {}", labels.join(", ")));
    };
    let note = task_kind
        .filter(|kind| *kind != agent.as_str() && is_launchable(kind))
        .map(|kind| {
            let (theirs, ours) = (agent_label(kind), agent_label(agent));
            if has_one_shot(kind) {
                format!("{theirs} is not installed, so {ours} drafted it")
            } else {
                format!("{theirs} cannot answer once, so {ours} drafted it")
            }
        });
    DraftPick::Agent {
        agent: agent.clone(),
        note,
    }
}

/// The agent a draft names when no agent runs (an untrusted repo, none installed): the first
/// candidate, as before the installed check.
pub fn first_draft_agent(task_kind: Option<&str>) -> String {
    draft_agents(task_kind)
        .into_iter()
        .next()
        .unwrap_or_else(|| "claude-code".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn program(agent: &str) -> String {
        format!("my-{agent} --flag")
    }

    #[test]
    fn puts_the_task_agent_first_then_the_answer_only_agents_once_each() {
        let agents = draft_agents(Some("codex"));
        assert_eq!(agents[..3], ["codex", "claude-code", "gemini"]);
        assert_eq!(agents.iter().filter(|agent| *agent == "codex").count(), 1);
        assert!(
            !agents
                .iter()
                .any(|agent| agent == "aider" || agent == "opencode")
        );
        assert_eq!(draft_agents(Some("aider"))[0], "claude-code");
        assert_eq!(draft_agents(None)[0], "claude-code");
        assert_eq!(first_draft_agent(Some("aider")), "claude-code");
        assert_eq!(first_draft_agent(Some("gemini")), "gemini");
    }

    #[test]
    fn picks_the_task_agent_when_it_is_installed() {
        let pick = pick_draft_agent(Some("codex"), &program, &|_| true);
        let expected = DraftPick::Agent {
            agent: "codex".into(),
            note: None,
        };
        assert_eq!(pick, expected);
    }

    #[test]
    fn says_why_another_agent_drafted() {
        let only = |wanted: &'static str| move |line: &str| line.starts_with(wanted);
        let pick = pick_draft_agent(Some("aider"), &program, &only("my-codex "));
        let expected = DraftPick::Agent {
            agent: "codex".into(),
            note: Some("Aider cannot answer once, so Codex drafted it".into()),
        };
        assert_eq!(pick, expected);
        let pick = pick_draft_agent(Some("codex"), &program, &only("my-claude-code "));
        let expected = DraftPick::Agent {
            agent: "claude-code".into(),
            note: Some("Codex is not installed, so Claude Code drafted it".into()),
        };
        assert_eq!(pick, expected);
    }

    #[test]
    fn names_no_task_agent_that_is_not_an_agent() {
        for kind in [None, Some("shell")] {
            let pick = pick_draft_agent(kind, &program, &|_| true);
            let expected = DraftPick::Agent {
                agent: "claude-code".into(),
                note: None,
            };
            assert_eq!(pick, expected);
        }
    }

    #[test]
    fn lists_the_agents_it_looked_for_when_none_is_installed() {
        let DraftPick::Missing(reason) = pick_draft_agent(None, &program, &|_| false) else {
            panic!("an agent was picked");
        };
        let start = format!("{NO_DRAFT_AGENT}: Claude Code, Codex, Gemini CLI");
        assert!(reason.starts_with(&start), "{reason}");
        assert!(!reason.contains("Aider"));
    }
}
