//! Port of `packages/web/src/lib/agents.ts`: the agents Natamux can start, in the order the
//! pickers show them. The ids are the daemon's.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{PoisonError, RwLock};

use natamux_proto::agents::custom::{CustomAgent, OTHER_BASE};

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
    icon_path_in(id, light, &custom_agents())
}

/// As `agent_icon_path`; a custom agent has the icon of its base, and "Other CLI" none.
pub fn icon_path_in(id: Option<&str>, light: bool, custom: &[CustomAgent]) -> Option<String> {
    let id = id.map(|id| base_in(id, custom));
    let icon = agent_icon(id)?;
    let light_one = format!("icons/agents/{icon}_light.svg");
    if light && crate::app::asset_table::find(&light_one).is_some() {
        return Some(light_one);
    }
    Some(format!("icons/agents/{icon}.svg"))
}

/// The custom agents the daemon holds, read when the dashboard starts and after a save. A static,
/// since the badges and menus that need it have no handle on the dashboard.
static CUSTOM: RwLock<Vec<CustomAgent>> = RwLock::new(Vec::new());

pub fn custom_agents() -> Vec<CustomAgent> {
    CUSTOM
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .clone()
}

/// How many times the list was set: 0 until the daemon's list came, once read or saved.
static SET_COUNT: AtomicU64 = AtomicU64::new(0);

/// Sets the list the daemon answered (a read or a save).
pub fn set_custom_agents(list: Vec<CustomAgent>) {
    *CUSTOM.write().unwrap_or_else(PoisonError::into_inner) = list;
    SET_COUNT.fetch_add(1, Ordering::SeqCst);
}

/// Taken before a read: the read applies only if the list was not set since, so an old read
/// never comes back over a newer save.
pub fn custom_agents_set_count() -> u64 {
    SET_COUNT.load(Ordering::SeqCst)
}

/// Whether the list came from the daemon. Until then a save would write an empty list over the
/// saved one.
pub fn custom_agents_known() -> bool {
    custom_agents_set_count() > 0
}

/// What an agent id behaves as: itself, or for a custom agent its base ("other" for any CLI).
pub fn base_in<'a>(id: &'a str, custom: &'a [CustomAgent]) -> &'a str {
    match custom.iter().find(|agent| agent.id == id) {
        Some(agent) if find(Some(id)).is_none() => &agent.base,
        _ => id,
    }
}

/// (id, label) of the built-in agents, then the custom ones, labelled by their id.
pub fn pick_list(custom: &[CustomAgent]) -> Vec<(String, String)> {
    let built_in = AGENT_CHOICES
        .iter()
        .map(|choice| (choice.id.to_owned(), choice.label.to_owned()));
    let own = custom
        .iter()
        .map(|agent| (agent.id.clone(), agent.id.clone()));
    built_in.chain(own).collect()
}

/// A task or stage with this agent can start: a built-in agent, or a custom one.
pub fn can_start_in(id: Option<&str>, custom: &[CustomAgent]) -> bool {
    is_agent_id(id) || custom.iter().any(|agent| Some(agent.id.as_str()) == id)
}

pub fn is_startable(id: Option<&str>) -> bool {
    can_start_in(id, &custom_agents())
}

/// The id `can_run_workflow` takes: a custom agent on a built-in base counts as the base; one on
/// "Other CLI" as itself, so it is refused.
pub fn workflow_agent<'a>(id: &'a str, custom: &'a [CustomAgent]) -> &'a str {
    match base_in(id, custom) {
        OTHER_BASE => id,
        base => base,
    }
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

    fn custom(id: &str, base: &str) -> CustomAgent {
        CustomAgent {
            id: id.into(),
            base: base.into(),
            command: id.into(),
        }
    }

    #[test]
    fn a_custom_agent_follows_its_base() {
        let list = [
            custom("claude-qwen", "claude-code"),
            custom("mine", OTHER_BASE),
        ];
        assert_eq!(base_in("claude-qwen", &list), "claude-code");
        assert_eq!(base_in("codex", &list), "codex");
        assert_eq!(workflow_agent("claude-qwen", &list), "claude-code");
        assert_eq!(workflow_agent("mine", &list), "mine");
        assert_eq!(
            icon_path_in(Some("claude-qwen"), false, &list).as_deref(),
            Some("icons/agents/claude.svg")
        );
        assert_eq!(icon_path_in(Some("mine"), false, &list), None);
    }

    #[test]
    fn custom_agents_come_after_the_built_in_ones_and_can_start() {
        let list = [custom("claude-qwen", "claude-code")];
        let picks = pick_list(&list);
        assert_eq!(picks.len(), AGENT_CHOICES.len() + 1);
        assert_eq!(
            picks[0],
            ("claude-code".to_owned(), "Claude Code".to_owned())
        );
        assert_eq!(
            picks[5],
            ("claude-qwen".to_owned(), "claude-qwen".to_owned())
        );
        assert!(can_start_in(Some("claude-qwen"), &list));
        assert!(!can_start_in(Some("claude-qwen"), &[]));
        assert!(can_start_in(Some("codex"), &[]));
        assert!(!can_start_in(None, &list));
    }

    // The only test that sets the list (it is the process's).
    #[test]
    fn knows_the_list_once_set_and_counts_each_set() {
        let before = custom_agents_set_count();
        set_custom_agents(vec![custom("claude-qwen", "claude-code")]);
        assert!(custom_agents_known());
        assert_eq!(custom_agents_set_count(), before + 1);
        assert!(is_startable(Some("claude-qwen")));
    }

    #[test]
    fn matches_the_agents_the_daemon_can_launch() {
        let ids: Vec<_> = AGENT_CHOICES.iter().map(|choice| choice.id).collect();
        assert_eq!(ids, natamux_proto::agents::start::LAUNCHABLE_AGENTS);
        assert!(is_agent_id(Some(DEFAULT_AGENT)));
        assert!(!is_agent_id(None));
    }
}
