//! Steps the tests of the pipeline editor rules share (`pipeline-editor-fixtures.ts`).

use natamux_proto::stages::resolve::step_refs_of;
use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, PathRef, Stage, StageBranch, StageGroup, StageKind, Step,
    StepRef,
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
            stage_step("implement", StageKind::Agent, Gate::Auto),
            stage_step("testing", StageKind::Testing, Gate::Auto),
        ],
    };
    let review = BranchPath {
        id: "pr-review".into(),
        name: Some("Review PR".into()),
        when: Some(Condition::Command {
            run: "test -f .no-review".into(),
        }),
        not: Some(true),
        stages: vec![stage_step("review", StageKind::Review, Gate::Manual)],
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
    branch_refs(stage_refs(small), stage_refs(review))
}

fn stage_refs(ids: &[&str]) -> Vec<StepRef> {
    ids.iter().map(|id| sid(id)).collect()
}

/// `branch_ref` with stages and groups in its paths.
pub fn branch_refs(small: Vec<StepRef>, review: Vec<StepRef>) -> StepRef {
    let path = |id: &str, stages: Vec<StepRef>| PathRef {
        id: id.to_string(),
        stages,
    };
    StepRef::Branch {
        id: "after-plan".into(),
        paths: vec![path("small", small), path("pr-review", review)],
    }
}

/// `branched` with Implement and Docs side by side (group "build") at the start of the Small
/// change path, then Testing.
pub fn path_grouped() -> Vec<Step> {
    let mut steps = branched();
    if let Some(Step::Branch(branch)) = steps.get_mut(2) {
        branch.paths[0].stages[0] = Step::Group(StageGroup {
            id: "build".into(),
            name: None,
            parallel: vec![
                Stage::new("implement", StageKind::Agent, Gate::Auto),
                Stage::new("docs", StageKind::Agent, Gate::Auto),
            ],
        });
    }
    steps
}
