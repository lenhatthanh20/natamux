//! The answer of `GET /tasks/:id/ended-shells` (`readEndedShells` in
//! `packages/daemon/src/http/routes/terminals.ts`, `endedShellSchema` in core): the last screen of
//! each shell of a done or cancelled task.

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EndedShell {
    pub name: String,
    /// None: a plain shell.
    pub agent: Option<String>,
    #[serde(default)]
    pub stage: Option<String>,
    #[serde(default)]
    pub session: Option<String>,
    pub ended_at: String,
    pub text: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize)]
pub struct EndedShellList {
    #[serde(default)]
    pub shells: Vec<EndedShell>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_an_ended_agent_shell_and_a_plain_one() {
        let list: EndedShellList = serde_json::from_str(
            r#"{"shells":[
                {"name":"1","agent":"claude-code","stage":"spec","session":"s-1","endedAt":"2026-10-22T00:00:00.000Z","text":"bye","later":1},
                {"name":"2","agent":null,"endedAt":"2026-10-22T00:00:01.000Z","text":""}
            ]}"#,
        )
        .unwrap();
        assert_eq!(list.shells[0].stage.as_deref(), Some("spec"));
        assert_eq!(list.shells[0].ended_at, "2026-10-22T00:00:00.000Z");
        assert_eq!(list.shells[1].agent, None);
        assert_eq!(list.shells[1].session, None);
    }

    #[test]
    fn no_shells_is_an_empty_list() {
        let list: EndedShellList = serde_json::from_str("{}").unwrap();
        assert!(list.shells.is_empty());
    }
}
