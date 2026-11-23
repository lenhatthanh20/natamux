//! Repo trust: the commands a project runs on this machine, which the user approves once and again
//! after each change (`GET /repos/:id/trust`, `POST /repos/:id/trust`).

use serde::{Deserialize, Serialize};

/// What a blocked stage says when the project's commands are not approved.
pub const UNTRUSTED_REASON: &str =
    "Repo commands are not approved. Review them in the project settings, then retry.";

/// What a blocked launch says when the agent config of the task's worktree is not the approved
/// one.
pub const CONFIG_CHANGED_REASON: &str =
    "The agent config in this worktree changed since it was approved. Review it, then retry.";

/// The note the approval shows: approval also turns on the agents' own trust of the folder.
pub const TRUST_NOTE: &str = concat!(
    "Agents will trust this folder and run in auto mode, so the repo's own agent config ",
    "(.codex, .cursor, .claude, MCP servers) runs too.",
);

/// One command of the project settings, and where it comes from (stage id and field).
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RepoCommand {
    #[serde(rename = "where")]
    pub place: String,
    pub text: String,
}

/// A setup script of the project folder, as text.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct SetupScript {
    pub file: String,
    pub text: String,
}

/// The answer of `GET /repos/:id/trust`. `hash` is what `POST /repos/:id/trust` must send back.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct RepoTrustView {
    pub trusted: bool,
    pub hash: String,
    pub commands: Vec<RepoCommand>,
    /// `.natamux-setup` and `.natamux-setup.ps1`, the ones that exist.
    #[serde(default)]
    pub setup: Vec<SetupScript>,
    /// Agent config files of the project folder (hooks, MCP servers), the ones that exist.
    #[serde(default, rename = "agentFiles")]
    pub agent_files: Vec<SetupScript>,
    /// The files that differ from the last approval: changed, new or gone.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub changed: Vec<String>,
    /// Why the agent config cannot be checked (a link, a path outside, a file over the caps).
    /// Such a folder can never be approved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unverifiable: Option<String>,
}

/// The body of `POST /repos/:id/trust`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct ApproveRepo {
    pub hash: String,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;

    #[test]
    fn names_the_place_of_a_command_where_on_the_wire() {
        let view = RepoTrustView {
            trusted: false,
            hash: "ab".into(),
            commands: vec![RepoCommand {
                place: "Stage test: check".into(),
                text: "pnpm test".into(),
            }],
            setup: Vec::new(),
            agent_files: vec![SetupScript {
                file: ".mcp.json".into(),
                text: "{}".into(),
            }],
            changed: Vec::new(),
            unverifiable: None,
        };
        let text = serde_json::to_string(&view).unwrap();
        assert_eq!(
            text,
            r#"{"trusted":false,"hash":"ab","commands":[{"where":"Stage test: check","text":"pnpm test"}],"setup":[],"agentFiles":[{"file":".mcp.json","text":"{}"}]}"#
        );
        assert_eq!(serde_json::from_str::<RepoTrustView>(&text).unwrap(), view);
    }

    #[test]
    fn names_the_changed_files_and_why_a_folder_cannot_be_checked() {
        let text = r#"{"trusted":false,"hash":"ab","commands":[],"changed":[".mcp.json"],"unverifiable":"x"}"#;
        let view: RepoTrustView = serde_json::from_str(text).unwrap();
        assert_eq!(view.changed, [".mcp.json"]);
        assert_eq!(view.unverifiable.as_deref(), Some("x"));
    }

    #[test]
    fn reads_a_view_without_agent_files() {
        let text = r#"{"trusted":true,"hash":"ab","commands":[],"setup":[]}"#;
        let view: RepoTrustView = serde_json::from_str(text).unwrap();
        assert!(view.agent_files.is_empty());
    }
}
