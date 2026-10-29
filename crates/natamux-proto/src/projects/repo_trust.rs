//! Repo trust: the commands a project runs on this machine, which the user approves once and again
//! after each change (`GET /repos/:id/trust`, `POST /repos/:id/trust`).

use serde::{Deserialize, Serialize};

/// What a blocked stage says when the project's commands are not approved.
pub const UNTRUSTED_REASON: &str =
    "Repo commands are not approved. Review them in the project settings, then retry.";

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
        };
        let text = serde_json::to_string(&view).unwrap();
        assert_eq!(
            text,
            r#"{"trusted":false,"hash":"ab","commands":[{"where":"Stage test: check","text":"pnpm test"}],"setup":[]}"#
        );
        assert_eq!(serde_json::from_str::<RepoTrustView>(&text).unwrap(), view);
    }
}
