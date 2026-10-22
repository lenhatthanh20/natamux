//! The answer of `GET /tasks/:id/terminals` (`listTerminals` in
//! `packages/daemon/src/http/routes/terminals.ts`, `terminalLaneSchema` in core).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer};

use crate::tasks::task::TaskState;

/// `SHELL_PROGRAMS` in `packages/core/src/terminals/terminal.ts`: a shell in front means nothing
/// else runs in the terminal.
pub const SHELL_PROGRAMS: [&str; 10] = [
    "bash",
    "zsh",
    "sh",
    "dash",
    "fish",
    "ksh",
    "nu",
    "pwsh",
    "powershell.exe",
    "cmd.exe",
];

/// The stage an agent terminal works or worked on, and that stage's state.
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct TerminalLane {
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub state: Option<TaskState>,
    /// Missing: not known. Some(None): a program that is no agent.
    #[serde(default, deserialize_with = "present")]
    pub agent: Option<Option<String>>,
}

impl TerminalLane {
    /// As the dashboard (`isAgentShell`): a lane with no `agent` field counts as an agent.
    pub fn is_agent(&self) -> bool {
        self.agent != Some(None)
    }
}

fn present<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<Option<String>>, D::Error> {
    Option::<String>::deserialize(deserializer).map(Some)
}

/// The open shells of a task. `restoring`: the shells that come back after a restart, with the
/// agent each comes back with (None: a plain shell).
#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct TerminalList {
    /// Names in the daemon's order.
    #[serde(default)]
    pub terminals: Vec<String>,
    #[serde(default)]
    pub agents: Vec<String>,
    #[serde(default)]
    pub lanes: BTreeMap<String, TerminalLane>,
    /// The program in front of each shell, None where the platform does not tell.
    #[serde(default)]
    pub programs: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub restoring: BTreeMap<String, Option<String>>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const ANSWER: &str = r#"{
        "terminals": ["1", "2", "5"], "agents": ["1", "2"],
        "lanes": {
            "1": {"stage":"implement","state":"running","agent":"claude-code"},
            "2": {"stage":null,"state":null,"agent":null},
            "5": {"stage":"spec","state":"done"}
        },
        "programs": {"1": null, "2": "node", "5": "zsh"},
        "restoring": {"2": "claude-code", "5": null}, "later": 1
    }"#;

    #[test]
    fn reads_terminals_lanes_programs_and_restoring() {
        let list: TerminalList = serde_json::from_str(ANSWER).unwrap();
        assert_eq!(list.terminals, ["1", "2", "5"]);
        assert_eq!(list.lanes["1"].state, Some(TaskState::Running));
        assert_eq!(list.programs["2"].as_deref(), Some("node"));
        assert_eq!(list.programs["1"], None);
        assert_eq!(list.restoring["2"].as_deref(), Some("claude-code"));
        assert_eq!(list.restoring["5"], None);
    }

    #[test]
    fn a_lane_with_agent_null_is_no_agent_and_a_missing_agent_counts_as_one() {
        let list: TerminalList = serde_json::from_str(ANSWER).unwrap();
        assert!(list.lanes["1"].is_agent());
        assert!(!list.lanes["2"].is_agent());
        assert!(list.lanes["5"].is_agent());
    }

    #[test]
    fn missing_parts_default_to_empty() {
        let list: TerminalList = serde_json::from_str(r#"{"terminals":["1"]}"#).unwrap();
        assert!(list.lanes.is_empty() && list.programs.is_empty() && list.restoring.is_empty());
    }
}
