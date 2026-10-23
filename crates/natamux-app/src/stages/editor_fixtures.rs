//! Steps the tests of the pipeline editor rules share (`pipeline-editor-fixtures.ts`).

use natamux_proto::stages::resolve::step_refs_of;
use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, PathRef, Stage, StageBranch, StageKind, Step, StepRef,
};

pub fn ids(steps: &[Step]) -> Vec<StepRef> {
    step_refs_of(steps)
}

pub fn stage_step(id: &str, kind: StageKind, gate: Gate) -> Step {
    Step::Stage(Stage::new(id, kind, gate))
}

/// A stage id as a task keeps it.
pub fn sid(id: &str) -> StepRef {
    StepRef::Stage(id.to_string())
}

pub fn group_ref(id: &str, stages: &[&str]) -> StepRef {
    StepRef::Group {
        id: id.to_string(),
        parallel: stages.iter().map(|stage| stage.to_string()).collect(),
    }
}

/// Context, Plan, a branch of two paths, Release.
pub fn branched() -> Vec<Step> {
    let small = BranchPath {
        id: "small".into(),
        name: Some("Small change".into()),
        when: Some(Condition::Output {
            file: "plan.md".into(),
            section: Some("Size".into()),
            contains: "small".into(),
        }),
        not: None,
        stages: vec![
            Stage::new("implement", StageKind::Agent, Gate::Auto),
            Stage::new("testing", StageKind::Testing, Gate::Auto),
        ],
    };
    let review = BranchPath {
        id: "pr-review".into(),
        name: Some("Review PR".into()),
        when: Some(Condition::Command {
            run: "test -f .no-review".into(),
        }),
        not: Some(true),
        stages: vec![Stage::new("review", StageKind::Review, Gate::Manual)],
    };
    vec![
        stage_step("context", StageKind::Context, Gate::Manual),
        stage_step("plan", StageKind::Agent, Gate::Manual),
        Step::Branch(StageBranch {
            id: "after-plan".into(),
            name: Some("After plan".into()),
            paths: vec![small, review],
        }),
        stage_step("release", StageKind::Release, Gate::Manual),
    ]
}

/// The branch of `branched` as a task keeps it, with these stages in its two paths.
pub fn branch_ref(small: &[&str], review: &[&str]) -> StepRef {
    let path = |id: &str, stages: &[&str]| PathRef {
        id: id.to_string(),
        stages: stages.iter().map(|stage| stage.to_string()).collect(),
    };
    StepRef::Branch {
        id: "after-plan".into(),
        paths: vec![path("small", small), path("pr-review", review)],
    }
}
