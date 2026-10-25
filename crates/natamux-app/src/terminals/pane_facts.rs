//! What each terminal pane's header says, with no gpui (`ShellPanes.tsx`, `lib/task-shells.ts`):
//! which terminals run agents, the state text and its mark, the hint about another terminal,
//! and the cover of a terminal that comes back after a restart.

use std::collections::BTreeMap;

use natamux_proto::agents::activity::TaskActivity;
use natamux_proto::tasks::task::{Task, TaskState};
use natamux_proto::terminals::list::{TerminalLane, TerminalList};

use super::pane_rules::{PaneHint, other_terminal_hint, pane_state_text};
use crate::agents::agent_choices::agent_label;
use crate::sidebar::shell_rows::terminal_state;

/// The agent lanes of a task's terminals (`agentLanes`): every agent shell, with its lane or an
/// empty one.
pub fn agent_lanes(list: &TerminalList) -> BTreeMap<String, TerminalLane> {
    let mut lanes: BTreeMap<String, TerminalLane> = list
        .lanes
        .iter()
        .filter(|(_, lane)| lane.is_agent())
        .map(|(name, lane)| (name.clone(), lane.clone()))
        .collect();
    for name in &list.agents {
        lanes.entry(name.clone()).or_default();
    }
    lanes
}

/// "Restoring Claude Code…" or "Restoring terminal…" for a terminal that comes back.
pub fn restoring_label(list: &TerminalList, name: &str) -> Option<String> {
    let agent = list.restoring.get(name)?;
    Some(match agent {
        Some(agent) => format!("Restoring {}\u{2026}", agent_label(Some(agent))),
        None => "Restoring terminal\u{2026}".into(),
    })
}

/// One pane's header facts, with no gpui.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaneFacts {
    pub name: String,
    /// Some: an agent shell, with its state when known.
    pub agent_state: Option<Option<TaskState>>,
    pub agent: Option<String>,
    pub state_text: Option<(String, bool)>,
    pub hint: Option<PaneHint>,
    pub closable: bool,
}

/// The last line of terminal `name`, read where the dashboard holds it.
fn activity_text<'a>(activity: Option<&'a TaskActivity>, name: &str) -> Option<&'a str> {
    Some(activity?.get(name)?.text.as_str())
}

/// The header of each pane. `shown` is how many agent panes are on screen: the needs-input text
/// shows only beside another agent pane, so it says which one waits.
pub fn pane_facts(
    task: &Task,
    list: &TerminalList,
    names: &[String],
    activity: Option<&TaskActivity>,
    shown_agents: usize,
) -> Vec<PaneFacts> {
    let lanes = agent_lanes(list);
    names
        .iter()
        .map(|name| {
            let lane = lanes.get(name);
            let state = lane.and_then(|lane| terminal_state(task, lane));
            let agent =
                lane.map(|lane| lane.agent.clone().flatten().or_else(|| task.agent.clone()));
            let state_text = state.and_then(|state| {
                let waits = state == TaskState::NeedsInput;
                if waits && shown_agents < 2 {
                    return None;
                }
                let text = pane_state_text(state, activity_text(activity, name));
                Some((text, waits))
            });
            let hint = (name == "1" && lane.is_none())
                .then(|| other_terminal_hint(task, &lanes))
                .flatten();
            PaneFacts {
                name: name.clone(),
                agent_state: lane.map(|_| state),
                agent: agent.flatten(),
                state_text,
                hint,
                closable: name != "1",
            }
        })
        .collect()
}

/// The pane that takes the focus when the focused one closes: the next one, else the one before.
pub fn focus_after_close(names: &[String], closed: &str) -> Option<String> {
    let at = names.iter().position(|name| name == closed)?;
    names
        .get(at + 1)
        .or_else(|| at.checked_sub(1).and_then(|before| names.get(before)))
        .cloned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn task(value: serde_json::Value) -> Task {
        let mut base = json!({
            "id": "T-1", "title": "t", "state": "running", "stage": "implement",
            "agent": "claude-code", "updatedAt": "2026-10-22T00:00:00.000Z"
        });
        if let (Some(base), Some(extra)) = (base.as_object_mut(), value.as_object()) {
            base.extend(extra.clone());
        }
        serde_json::from_value(base).unwrap()
    }

    fn list(value: serde_json::Value) -> TerminalList {
        serde_json::from_value(value).unwrap()
    }

    fn names(names: &[&str]) -> Vec<String> {
        names.iter().map(|name| name.to_string()).collect()
    }

    #[test]
    fn every_agent_shell_has_a_lane_and_a_program_has_none() {
        let shells = list(json!({
            "terminals": ["1", "2", "5"], "agents": ["1"],
            "lanes": { "5": { "stage": "spec", "state": "done" }, "2": { "agent": null } }
        }));
        let lanes = agent_lanes(&shells);
        assert_eq!(lanes.keys().collect::<Vec<_>>(), ["1", "5"]);
        assert_eq!(lanes["1"], TerminalLane::default());
    }

    #[test]
    fn names_what_comes_back_after_a_restart() {
        let shells = list(json!({ "restoring": { "1": "copilot", "3": null } }));
        assert_eq!(
            restoring_label(&shells, "1").as_deref(),
            Some("Restoring GitHub Copilot\u{2026}")
        );
        assert_eq!(
            restoring_label(&shells, "3").as_deref(),
            Some("Restoring terminal\u{2026}")
        );
        assert_eq!(restoring_label(&shells, "2"), None);
    }

    #[test]
    fn marks_a_waiting_agent_only_beside_another_agent_pane() {
        let waiting = task(json!({ "state": "needs-input" }));
        let shells = list(json!({
            "terminals": ["1", "2"], "agents": ["1", "2"],
            "lanes": { "1": { "stage": "implement" }, "2": { "stage": "test-spec", "state": "running", "agent": "codex" } }
        }));
        let alone = pane_facts(&waiting, &shells, &names(&["1"]), None, 1);
        assert_eq!(alone[0].state_text, None);
        let activity: TaskActivity = serde_json::from_value(json!({
            "2": { "text": "Reading the prompt", "at": "2026-10-22T00:00:00.000Z" }
        }))
        .unwrap();
        let both = pane_facts(&waiting, &shells, &names(&["1", "2"]), Some(&activity), 2);
        assert_eq!(both[0].state_text, Some(("Needs input".into(), true)));
        assert_eq!(
            both[1].state_text,
            Some(("Running \u{b7} Reading the prompt".into(), false))
        );
        assert_eq!(both[1].agent.as_deref(), Some("codex"));
        assert_eq!(both[0].agent.as_deref(), Some("claude-code"));
        assert!(!both[0].closable && both[1].closable);
    }

    #[test]
    fn terminal_1_with_no_agent_says_what_another_terminal_does() {
        let own_shell = task(json!({ "stage": "spec" }));
        let shells = list(json!({
            "terminals": ["1", "5"], "agents": ["5"],
            "lanes": { "5": { "stage": "spec", "agent": "claude-code" } }
        }));
        let facts = pane_facts(&own_shell, &shells, &names(&["1", "5"]), None, 1);
        assert_eq!(facts[0].agent_state, None);
        assert_eq!(
            facts[0].hint.as_ref().map(|hint| hint.text.as_str()),
            Some("Terminal 5 is working")
        );
        assert_eq!(facts[1].agent_state, Some(Some(TaskState::Running)));
        assert_eq!(facts[1].hint, None);
    }

    #[test]
    fn closing_the_focused_pane_moves_the_focus_to_the_next_else_the_one_before() {
        let shown = names(&["1", "5", "8"]);
        assert_eq!(focus_after_close(&shown, "5"), Some("8".into()));
        assert_eq!(focus_after_close(&shown, "8"), Some("5".into()));
        assert_eq!(focus_after_close(&names(&["1"]), "1"), None);
        assert_eq!(focus_after_close(&shown, "3"), None);
    }
}
