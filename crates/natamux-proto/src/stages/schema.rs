//! The stages of a pipeline (`packages/core/src/stages/schema.ts`): stages, groups that run side by
//! side, and branches of exclusive paths. Only what the app reads; a stage's prompt is left out.

use serde::{Deserialize, Serialize};

/// `STAGE_KINDS`.
pub const STAGE_KINDS: [&str; 6] = ["context", "agent", "review", "testing", "release", "manual"];
/// `GROUP_KINDS`: the kinds that may run side by side in a group.
pub const GROUP_KINDS: [StageKind; 2] = [StageKind::Agent, StageKind::Manual];
/// `GATES`.
pub const GATES: [&str; 2] = ["manual", "auto"];

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum StageKind {
    Context,
    Agent,
    Review,
    Testing,
    Release,
    #[default]
    Manual,
}

impl StageKind {
    /// The word of the file: "context", "agent", ...
    pub fn as_str(self) -> &'static str {
        match self {
            StageKind::Context => "context",
            StageKind::Agent => "agent",
            StageKind::Review => "review",
            StageKind::Testing => "testing",
            StageKind::Release => "release",
            StageKind::Manual => "manual",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Gate {
    #[default]
    Manual,
    Auto,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub id: String,
    pub kind: StageKind,
    pub gate: Gate,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_commit: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub require_clean: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub done_when: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ci: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub own_shell: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_shell: Option<bool>,
}

impl Stage {
    /// A stage of this id and kind with no other field set.
    pub fn new(id: &str, kind: StageKind, gate: Gate) -> Stage {
        Stage {
            id: id.into(),
            kind,
            gate,
            ..Stage::default()
        }
    }

    /// `isOn`: a stage that is off stays in the file, but new tasks do not get it.
    pub fn is_on(&self) -> bool {
        self.enabled != Some(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StageGroup {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub parallel: Vec<Stage>,
}

/// What picks a path of a branch: a stage file has a text, or a command exits with code 0.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum Condition {
    Output {
        file: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        section: Option<String>,
        contains: String,
    },
    Command {
        run: String,
    },
}

/// One way through a branch. No `when`: the Otherwise path. `not` takes it when the condition
/// does not hold.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct BranchPath {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<Condition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub not: Option<bool>,
    pub stages: Vec<Stage>,
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct StageBranch {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub paths: Vec<BranchPath>,
}

/// A step of the pipeline, in `stepSchema` order: a group, a branch, or one stage.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Step {
    Group(StageGroup),
    Branch(StageBranch),
    Stage(Stage),
}

impl Step {
    /// `isGroup`.
    pub fn is_group(&self) -> bool {
        matches!(self, Step::Group(_))
    }

    /// `isBranch`.
    pub fn is_branch(&self) -> bool {
        matches!(self, Step::Branch(_))
    }

    /// `isStage`.
    pub fn is_stage(&self) -> bool {
        matches!(self, Step::Stage(_))
    }

    pub fn id(&self) -> &str {
        match self {
            Step::Group(group) => &group.id,
            Step::Branch(branch) => &branch.id,
            Step::Stage(stage) => &stage.id,
        }
    }

    pub fn name(&self) -> Option<&str> {
        match self {
            Step::Group(group) => group.name.as_deref(),
            Step::Branch(branch) => branch.name.as_deref(),
            Step::Stage(stage) => stage.name.as_deref(),
        }
    }

    /// `stagesOf`: the stage itself, the stages of a group, or the stages of every path.
    pub fn stages(&self) -> Vec<&Stage> {
        match self {
            Step::Group(group) => group.parallel.iter().collect(),
            Step::Branch(branch) => branch.paths.iter().flat_map(|path| &path.stages).collect(),
            Step::Stage(stage) => vec![stage],
        }
    }
}

/// A path of a branch as a task keeps it: its id and stage ids.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
pub struct PathRef {
    pub id: String,
    pub stages: Vec<String>,
}

/// A step of a task's pipeline as the task keeps it (`StepRef` in core): a stage id, a group, or
/// a branch of paths.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(untagged)]
pub enum StepRef {
    Stage(String),
    Group { id: String, parallel: Vec<String> },
    Branch { id: String, paths: Vec<PathRef> },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_three_kinds_of_step() {
        let steps: Vec<Step> = serde_json::from_str(
            r#"[
              {"id":"spec","kind":"agent","gate":"manual","output":"spec.md","prompt":"x"},
              {"id":"build","parallel":[
                {"id":"test-spec","kind":"agent","gate":"auto"},
                {"id":"implement","kind":"agent","gate":"auto","workflow":true}]},
              {"id":"kind","name":"Bug or feature","paths":[
                {"id":"bug","when":{"type":"command","run":"grep -q bug x"},
                 "stages":[{"id":"repro","kind":"agent","gate":"auto"}]},
                {"id":"feature","stages":[{"id":"plan","kind":"agent","gate":"manual"}]}]}
            ]"#,
        )
        .unwrap();
        assert!(
            matches!(&steps[0], Step::Stage(stage) if stage.output.as_deref() == Some("spec.md"))
        );
        assert!(matches!(&steps[1], Step::Group(group) if group.parallel.len() == 2));
        let Step::Branch(branch) = &steps[2] else {
            panic!("not a branch")
        };
        assert_eq!(
            branch.paths[0].when,
            Some(Condition::Command {
                run: "grep -q bug x".into()
            })
        );
        assert_eq!(steps[2].stages().len(), 2);
        assert_eq!(steps[2].name(), Some("Bug or feature"));
    }

    #[test]
    fn keeps_the_prompt_of_a_stage_and_names_the_kind_of_a_step() {
        let steps: Vec<Step> = serde_json::from_str(
            r#"[{"id":"spec","kind":"agent","gate":"manual","prompt":"Do it"}]"#,
        )
        .unwrap();
        assert!(
            matches!(&steps[0], Step::Stage(stage) if stage.prompt.as_deref() == Some("Do it"))
        );
        let again = serde_json::to_value(&steps).unwrap();
        assert_eq!(again[0]["prompt"], "Do it");
        assert!(steps[0].is_stage() && !steps[0].is_group() && !steps[0].is_branch());
        assert_eq!(GROUP_KINDS.map(StageKind::as_str), ["agent", "manual"]);
        for (kind, word) in [
            (StageKind::Context, "context"),
            (StageKind::Testing, "testing"),
        ] {
            assert!(STAGE_KINDS.contains(&kind.as_str()) && kind.as_str() == word);
        }
    }

    #[test]
    fn reads_the_step_refs_of_a_task() {
        let refs: Vec<StepRef> = serde_json::from_str(
            r#"["context",{"id":"build","parallel":["a","b"]},
                {"id":"kind","paths":[{"id":"bug","stages":["repro"]}]}]"#,
        )
        .unwrap();
        assert_eq!(refs[0], StepRef::Stage("context".into()));
        assert!(matches!(&refs[1], StepRef::Group { parallel, .. } if parallel.len() == 2));
        assert!(matches!(&refs[2], StepRef::Branch { paths, .. } if paths[0].stages == ["repro"]));
    }
}
