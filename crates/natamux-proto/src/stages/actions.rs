//! What the stage routes take and send (`packages/daemon/src/stages/stage-actions.ts`,
//! `http/routes/stages.ts`, `stages/stage-files.ts`).

use serde::{Deserialize, Serialize};

/// `STAGE_ACTIONS`.
pub const STAGE_ACTIONS: [&str; 7] = [
    "approve", "reopen", "skip", "pass", "fail", "report", "start",
];

/// Longest `stage` and `to` of a stage action, and of a toggle's `stage`.
pub const MAX_STAGE_ID_LENGTH: usize = 40;
/// Longest `reason` of a stage action, after trimming.
pub const MAX_REASON_LENGTH: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StageAction {
    Approve,
    Reopen,
    Skip,
    Pass,
    Fail,
    Report,
    Start,
}

/// The body of `POST /tasks/:id/stage`. `to`: the stage a `reopen` goes back to. `reason`: why a
/// `reopen` or `fail`, for the agent and the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageActionInput {
    pub action: StageAction,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub to: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl StageActionInput {
    pub fn new(action: StageAction) -> StageActionInput {
        StageActionInput {
            action,
            stage: None,
            to: None,
            reason: None,
        }
    }
}

/// The body of `PUT /tasks/:id/workflow`, `/ci` and `/skip`.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct StageToggle {
    pub stage: String,
    pub on: bool,
}

/// A file a stage wrote, as `GET /tasks/:id/files` lists it.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StageFile {
    pub name: String,
    pub size: u64,
    pub modified_at: String,
}

/// The answer of `GET /tasks/:id/files`.
#[derive(Clone, Debug, Default, Deserialize)]
pub struct StageFileList {
    #[serde(default)]
    pub files: Vec<StageFile>,
}

/// The answer of `GET` and `PUT /tasks/:id/files/:name`. The PUT body is `FileText`.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
pub struct StageFileText {
    pub name: String,
    pub text: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sends_an_action_with_only_the_fields_that_are_set() {
        let mut input = StageActionInput::new(StageAction::Reopen);
        input.to = Some("spec".into());
        input.reason = Some("Add cases".into());
        assert_eq!(
            serde_json::to_string(&input).unwrap(),
            r#"{"action":"reopen","to":"spec","reason":"Add cases"}"#
        );
        assert_eq!(
            serde_json::to_string(&StageActionInput::new(StageAction::Approve)).unwrap(),
            r#"{"action":"approve"}"#
        );
    }

    #[test]
    fn names_every_action_like_the_daemon() {
        let actions = [
            StageAction::Approve,
            StageAction::Reopen,
            StageAction::Skip,
            StageAction::Pass,
            StageAction::Fail,
            StageAction::Report,
            StageAction::Start,
        ];
        let names: Vec<String> = actions
            .iter()
            .map(|action| serde_json::to_string(action).unwrap().replace('"', ""))
            .collect();
        assert_eq!(names, STAGE_ACTIONS);
    }

    #[test]
    fn sends_a_toggle_and_reads_the_files() {
        let toggle = StageToggle {
            stage: "plan".into(),
            on: true,
        };
        assert_eq!(
            serde_json::to_string(&toggle).unwrap(),
            r#"{"stage":"plan","on":true}"#
        );
        let list: StageFileList = serde_json::from_str(
            r#"{"files":[{"name":"spec.md","size":12,"modifiedAt":"2026-10-23T01:00:00.000Z","x":1}]}"#,
        )
        .unwrap();
        assert_eq!(list.files[0].name, "spec.md");
        assert_eq!(list.files[0].size, 12);
    }

    #[test]
    fn reads_the_text_of_a_stage_file() {
        let file: StageFileText =
            serde_json::from_str(r##"{"name":"spec.md","text":"# Spec\n"}"##).unwrap();
        assert_eq!(
            (file.name.as_str(), file.text.as_str()),
            ("spec.md", "# Spec\n")
        );
    }
}
