//! Port of `packages/web/src/lib/agents.ts`: the agents Natamux can start, in the order the
//! pickers show them. The ids are the daemon's.

pub struct AgentChoice {
    pub id: &'static str,
    pub label: &'static str,
    // Agent icons come with the agent SVGs.
    #[cfg_attr(not(test), allow(dead_code))]
    /// A material-icon-theme name, or one of the app's own icons.
    pub icon: &'static str,
}

pub const AGENT_CHOICES: [AgentChoice; 5] = [
    AgentChoice {
        id: "claude-code",
        label: "Claude Code",
        icon: "claude",
    },
    AgentChoice {
        id: "codex",
        label: "Codex",
        icon: "codex",
    },
    AgentChoice {
        id: "gemini",
        label: "Gemini CLI",
        icon: "gemini-ai",
    },
    AgentChoice {
        id: "copilot",
        label: "GitHub Copilot",
        icon: "copilot",
    },
    AgentChoice {
        id: "cursor",
        label: "Cursor",
        icon: "cursor",
    },
];

pub const DEFAULT_AGENT: &str = "claude-code";

fn find(id: Option<&str>) -> Option<&'static AgentChoice> {
    AGENT_CHOICES.iter().find(|choice| Some(choice.id) == id)
}

pub fn is_agent_id(value: Option<&str>) -> bool {
    find(value).is_some()
}

/// A known agent by name; an unknown id as it is; "No agent" for none.
pub fn agent_label(id: Option<&str>) -> String {
    match (find(id), id) {
        (Some(choice), _) => choice.label.to_string(),
        (None, Some(id)) => id.to_string(),
        (None, None) => "No agent".to_string(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn agent_icon(id: Option<&str>) -> Option<&'static str> {
    find(id).map(|choice| choice.icon)
}

/// The asset of an agent's icon. Some icons are dark on dark; the theme has a lighter one, made
/// for light editors, used in the light theme (`iconUrl`).
pub fn agent_icon_path(id: Option<&str>, light: bool) -> Option<String> {
    let icon = agent_icon(id)?;
    let light_one = format!("icons/agents/{icon}_light.svg");
    if light && crate::app::asset_table::find(&light_one).is_some() {
        return Some(light_one);
    }
    Some(format!("icons/agents/{icon}.svg"))
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn agent_icon_names() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = Vec::new();
    for choice in &AGENT_CHOICES {
        if !names.contains(&choice.icon) {
            names.push(choice.icon);
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_agent_icon_is_an_asset_in_both_themes() {
        for choice in &AGENT_CHOICES {
            for light in [false, true] {
                let path = agent_icon_path(Some(choice.id), light).unwrap();
                assert!(crate::app::asset_table::find(&path).is_some(), "{path}");
            }
        }
        assert_eq!(
            agent_icon_path(Some("copilot"), true).as_deref(),
            Some("icons/agents/copilot_light.svg")
        );
        assert_eq!(
            agent_icon_path(Some("claude-code"), true).as_deref(),
            Some("icons/agents/claude.svg")
        );
        assert_eq!(agent_icon_path(Some("aider"), false), None);
    }

    #[test]
    fn agent_icon_gets_its_icon() {
        for (id, icon) in [
            ("claude-code", "claude"),
            ("gemini", "gemini-ai"),
            ("copilot", "copilot"),
            ("cursor", "cursor"),
            ("codex", "codex"),
        ] {
            assert_eq!(agent_icon(Some(id)), Some(icon));
        }
    }

    #[test]
    fn has_no_icon_for_an_unknown_agent_or_no_agent() {
        assert_eq!(agent_icon(Some("my-agent")), None);
        assert_eq!(agent_icon(None), None);
    }

    #[test]
    fn every_known_agent_has_a_label_and_an_icon_of_its_own_not_the_generic_one() {
        for choice in &AGENT_CHOICES {
            assert!(!choice.label.trim().is_empty());
            assert_eq!(agent_label(Some(choice.id)), choice.label);
            assert_eq!(agent_icon(Some(choice.id)), Some(choice.icon));
        }
    }

    #[test]
    fn never_shares_an_icon_with_another_agent() {
        let icons: Vec<_> = AGENT_CHOICES.iter().map(|choice| choice.icon).collect();
        assert_eq!(agent_icon_names().len(), icons.len());
    }

    #[test]
    fn agent_icon_names_lists_each_icon_once() {
        assert_eq!(
            agent_icon_names(),
            ["claude", "codex", "gemini-ai", "copilot", "cursor"]
        );
    }

    #[test]
    fn names_a_known_agent_and_shows_an_unknown_id_as_it_is() {
        assert_eq!(agent_label(Some("claude-code")), "Claude Code");
        assert_eq!(agent_label(Some("my-agent")), "my-agent");
        assert_eq!(agent_label(None), "No agent");
    }

    #[test]
    fn treats_an_id_in_another_case_as_unknown() {
        assert_eq!(agent_label(Some("Claude-Code")), "Claude-Code");
        assert_eq!(agent_icon(Some("Claude-Code")), None);
    }

    #[test]
    fn matches_the_agents_the_daemon_can_launch() {
        let ids: Vec<_> = AGENT_CHOICES.iter().map(|choice| choice.id).collect();
        assert_eq!(ids, natamux_proto::agents::start::LAUNCHABLE_AGENTS);
        assert!(is_agent_id(Some(DEFAULT_AGENT)));
        assert!(!is_agent_id(None));
    }
}
