//! `resolveSteps`, `stepRefsOf` (`packages/core/src/stages/default-pipeline.ts`) and `activeSteps`
//! (`schema.ts`): a task keeps the ids of its steps; the settings and the catalog say the rest.

use std::collections::HashMap;

use crate::stages::catalog::catalog;
use crate::stages::schema::{BranchPath, PathRef, Stage, StageBranch, StageGroup, Step, StepRef};

/// `stepRefsOf`: what a task keeps of its pipeline, the ids in order.
pub fn step_refs_of(steps: &[Step]) -> Vec<StepRef> {
    steps
        .iter()
        .map(|step| match step {
            Step::Stage(stage) => StepRef::Stage(stage.id.clone()),
            Step::Group(group) => StepRef::Group {
                id: group.id.clone(),
                parallel: group.parallel.iter().map(|each| each.id.clone()).collect(),
            },
            Step::Branch(branch) => StepRef::Branch {
                id: branch.id.clone(),
                paths: branch
                    .paths
                    .iter()
                    .map(|path| PathRef {
                        id: path.id.clone(),
                        stages: path.stages.iter().map(|each| each.id.clone()).collect(),
                    })
                    .collect(),
            },
        })
        .collect()
}

/// `activeSteps`: the steps that new tasks get. Stages that are off are left out, and so is a
/// group or a path with none on. A branch left with only an Otherwise path runs its stages as
/// plain steps.
pub fn active_steps(steps: &[Step]) -> Vec<Step> {
    steps.iter().flat_map(active_of).collect()
}

fn active_of(step: &Step) -> Vec<Step> {
    match step {
        Step::Stage(stage) => {
            if stage.is_on() {
                vec![step.clone()]
            } else {
                Vec::new()
            }
        }
        Step::Group(group) => {
            let on: Vec<Stage> = group
                .parallel
                .iter()
                .filter(|each| each.is_on())
                .cloned()
                .collect();
            // The rules ask for two stages that are on; one left over runs on its own.
            match on.len() {
                0 => Vec::new(),
                1 => on.into_iter().map(Step::Stage).collect(),
                _ => vec![Step::Group(StageGroup {
                    parallel: on,
                    ..group.clone()
                })],
            }
        }
        Step::Branch(branch) => {
            let mut paths: Vec<BranchPath> = branch
                .paths
                .iter()
                .filter_map(|path| {
                    let on: Vec<Stage> = path
                        .stages
                        .iter()
                        .filter(|each| each.is_on())
                        .cloned()
                        .collect();
                    (!on.is_empty()).then(|| BranchPath {
                        stages: on,
                        ..path.clone()
                    })
                })
                .collect();
            if paths.len() == 1 && paths[0].when.is_none() {
                return paths
                    .remove(0)
                    .stages
                    .into_iter()
                    .map(Step::Stage)
                    .collect();
            }
            if paths.is_empty() {
                return Vec::new();
            }
            vec![Step::Branch(StageBranch {
                paths,
                ..branch.clone()
            })]
        }
    }
}

/// Turns a task's step ids back into steps, with each stage's settings from `pipeline`. A stage
/// that is gone from the settings keeps the catalog settings of its id; a stage of its own that is
/// gone becomes a manual stage, so the task can go on. A stage that is off still runs.
pub fn resolve_steps(refs: &[StepRef], pipeline: &[Step]) -> Vec<Step> {
    let mut known: HashMap<String, Stage> = HashMap::new();
    let from_settings = pipeline.iter().flat_map(Step::stages).cloned();
    for stage in catalog().iter().cloned().chain(from_settings) {
        known.insert(stage.id.clone(), stage);
    }
    let stage = |id: &str| {
        known
            .get(id)
            .cloned()
            .unwrap_or_else(|| Stage::new(id, Default::default(), Default::default()))
    };
    refs.iter()
        .map(|reference| match reference {
            StepRef::Stage(id) => Step::Stage(stage(id)),
            StepRef::Group { id, parallel } => Step::Group(StageGroup {
                id: id.clone(),
                name: name_of(pipeline, id),
                parallel: parallel.iter().map(|each| stage(each)).collect(),
            }),
            StepRef::Branch { id, paths } => {
                let now = pipeline.iter().find_map(|step| match step {
                    Step::Branch(branch) if branch.id == *id => Some(branch),
                    _ => None,
                });
                Step::Branch(StageBranch {
                    id: id.clone(),
                    name: name_of(pipeline, id),
                    paths: paths
                        .iter()
                        .map(|path| {
                            // Conditions come from the settings; a path that is gone from them
                            // keeps its stages and has no condition.
                            let kept = now.and_then(|branch| {
                                branch.paths.iter().find(|each| each.id == path.id)
                            });
                            BranchPath {
                                id: path.id.clone(),
                                name: kept.and_then(|each| each.name.clone()),
                                when: kept.and_then(|each| each.when.clone()),
                                not: kept.and_then(|each| each.not),
                                stages: path.stages.iter().map(|each| stage(each)).collect(),
                            }
                        })
                        .collect(),
                })
            }
        })
        .collect()
}

fn name_of(pipeline: &[Step], id: &str) -> Option<String> {
    pipeline
        .iter()
        .find(|step| step.id() == id)
        .and_then(|step| step.name().map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stages::catalog::default_pipeline;
    use crate::stages::schema::{Condition, Gate, StageKind};

    #[test]
    fn turns_the_step_ids_of_a_task_back_into_steps_with_their_settings() {
        let pipeline = default_pipeline();
        assert_eq!(resolve_steps(&step_refs_of(&pipeline), &pipeline), pipeline);
    }

    #[test]
    fn finds_a_stage_of_a_group_for_a_task_that_has_it_on_its_own() {
        let refs = [StepRef::Stage("implement".into())];
        let steps = resolve_steps(&refs, &default_pipeline());
        let mut implement = Stage::new("implement", StageKind::Agent, Gate::Auto);
        implement.workflow = Some(true);
        implement.ci = Some(true);
        assert_eq!(steps, [Step::Stage(implement)]);
    }

    #[test]
    fn makes_a_stage_of_its_own_that_is_gone_from_the_settings_a_manual_one() {
        let steps = resolve_steps(&[StepRef::Stage("sign-off".into())], &default_pipeline());
        assert_eq!(
            steps,
            [Step::Stage(Stage::new(
                "sign-off",
                StageKind::Manual,
                Gate::Manual
            ))]
        );
    }

    #[test]
    fn keeps_the_catalog_settings_of_a_catalog_stage_that_is_gone_from_the_settings() {
        let steps = resolve_steps(&[StepRef::Stage("security".into())], &default_pipeline());
        let Step::Stage(security) = &steps[0] else {
            panic!("not a stage")
        };
        assert_eq!(security.kind, StageKind::Agent);
        assert_eq!(security.output.as_deref(), Some("security.md"));
    }

    #[test]
    fn still_runs_a_stage_that_is_off_for_a_task_that_has_it() {
        let mut plan = Stage::new("plan", StageKind::Agent, Gate::Auto);
        plan.enabled = Some(false);
        let steps = resolve_steps(&[StepRef::Stage("plan".into())], &[Step::Stage(plan)]);
        let Step::Stage(plan) = &steps[0] else {
            panic!("not a stage")
        };
        assert_eq!((plan.gate, plan.enabled), (Gate::Auto, Some(false)));
    }

    #[test]
    fn keeps_the_default_settings_of_a_default_stage_that_is_gone_from_the_settings() {
        let only = [Step::Stage(Stage::new(
            "implement",
            StageKind::Agent,
            Gate::Auto,
        ))];
        let steps = resolve_steps(&[StepRef::Stage("plan".into())], &only);
        let Step::Stage(plan) = &steps[0] else {
            panic!("not a stage")
        };
        assert_eq!(plan.output.as_deref(), Some("plan.md"));
    }

    fn stage(id: &str, kind: StageKind, gate: Gate) -> Stage {
        Stage::new(id, kind, gate)
    }

    fn small() -> BranchPath {
        BranchPath {
            id: "small".into(),
            name: Some("Small change".into()),
            when: Some(Condition::Output {
                file: "plan.md".into(),
                section: Some("Size".into()),
                contains: "small".into(),
            }),
            not: None,
            stages: vec![
                stage("implement", StageKind::Agent, Gate::Auto),
                stage("testing", StageKind::Testing, Gate::Auto),
            ],
        }
    }

    fn pr_review() -> BranchPath {
        BranchPath {
            id: "pr-review".into(),
            name: Some("Review PR".into()),
            when: Some(Condition::Command {
                run: "test -f .no-review".into(),
            }),
            not: Some(true),
            stages: vec![stage("review", StageKind::Review, Gate::Manual)],
        }
    }

    #[test]
    fn keeps_the_ids_of_its_paths_in_a_task_and_reads_the_rest_from_the_settings() {
        let plan = stage("plan", StageKind::Agent, Gate::Manual);
        let release = stage("release", StageKind::Release, Gate::Manual);
        let branch = StageBranch {
            id: "after-plan".into(),
            name: Some("After plan".into()),
            paths: vec![small(), pr_review()],
        };
        let steps = vec![
            Step::Stage(plan.clone()),
            Step::Branch(branch),
            Step::Stage(release.clone()),
        ];
        let refs = step_refs_of(&steps);
        assert!(
            matches!(&refs[1], StepRef::Branch { paths, .. } if paths[0].stages == ["implement", "testing"])
        );
        assert_eq!(resolve_steps(&refs, &steps), steps);

        // The settings changed: a new condition counts, and a path that is gone keeps its stages,
        // with the catalog settings of a stage that is gone too.
        let mut not_small = small();
        not_small.not = Some(true);
        let now = vec![
            Step::Stage(plan),
            Step::Branch(StageBranch {
                id: "after-plan".into(),
                name: None,
                paths: vec![
                    not_small.clone(),
                    BranchPath {
                        id: "other".into(),
                        name: None,
                        when: None,
                        not: None,
                        stages: vec![stage("testing", StageKind::Testing, Gate::Auto)],
                    },
                ],
            }),
            Step::Stage(release),
        ];
        let mut review = stage("review", StageKind::Review, Gate::Manual);
        review.name = Some("Manual review".into());
        assert_eq!(
            resolve_steps(&refs, &now)[1],
            Step::Branch(StageBranch {
                id: "after-plan".into(),
                name: None,
                paths: vec![
                    not_small,
                    BranchPath {
                        id: "pr-review".into(),
                        name: None,
                        when: None,
                        not: None,
                        stages: vec![review],
                    },
                ],
            })
        );
    }

    #[test]
    fn runs_the_automated_tests_then_an_agent_review_and_waits_for_no_person() {
        let pipeline = default_pipeline();
        assert_eq!(
            step_refs_of(&pipeline),
            [
                StepRef::Stage("context".into()),
                StepRef::Stage("spec".into()),
                StepRef::Stage("plan".into()),
                StepRef::Group {
                    id: "build".into(),
                    parallel: vec!["test-spec".into(), "implement".into()]
                },
                StepRef::Stage("testing".into()),
                StepRef::Stage("agent-review".into()),
                StepRef::Stage("release".into()),
            ]
        );
        let Step::Stage(spec) = &pipeline[1] else {
            panic!("not a stage")
        };
        let mut want = Stage::new("spec", StageKind::Agent, Gate::Manual);
        want.output = Some("spec.md".into());
        want.sections = Some(
            [
                "Goal",
                "Scope",
                "Out of scope",
                "Acceptance criteria",
                "Code map",
            ]
            .map(String::from)
            .to_vec(),
        );
        assert_eq!(spec, &want);
        assert_eq!(active_steps(&pipeline), pipeline);
    }

    fn manual(id: &str, enabled: Option<bool>) -> Stage {
        let mut stage = Stage::new(id, StageKind::Agent, Gate::Manual);
        stage.enabled = enabled;
        stage
    }

    #[test]
    fn leaves_them_out_of_the_steps_of_a_new_task() {
        let group = |id: &str, parallel: Vec<Stage>| {
            Step::Group(StageGroup {
                id: id.into(),
                name: None,
                parallel,
            })
        };
        let steps = [
            Step::Stage(Stage::new("context", StageKind::Context, Gate::Manual)),
            Step::Stage(manual("plan", Some(false))),
            group(
                "build",
                vec![manual("a", Some(false)), manual("b", Some(false))],
            ),
            group(
                "checks",
                vec![
                    manual("c", None),
                    manual("d", None),
                    manual("e", Some(false)),
                ],
            ),
        ];
        assert_eq!(
            step_refs_of(&active_steps(&steps)),
            [
                StepRef::Stage("context".into()),
                StepRef::Group {
                    id: "checks".into(),
                    parallel: vec!["c".into(), "d".into()]
                },
            ]
        );
    }

    #[test]
    fn leaves_out_stages_and_paths_that_are_off_and_runs_a_lone_otherwise_path_as_steps() {
        let plan = Step::Stage(stage("plan", StageKind::Agent, Gate::Manual));
        let release = Step::Stage(stage("release", StageKind::Release, Gate::Manual));
        let off = |id: &str, kind: StageKind| {
            let mut each = stage(id, kind, Gate::Auto);
            each.enabled = Some(false);
            each
        };
        let implement = stage("implement", StageKind::Agent, Gate::Auto);
        let testing = stage("testing", StageKind::Testing, Gate::Auto);
        let review = stage("review", StageKind::Review, Gate::Manual);
        let when = small().when;
        let path = |id: &str, when: &Option<Condition>, stages: Vec<Stage>| BranchPath {
            id: id.into(),
            name: None,
            when: when.clone(),
            not: None,
            stages,
        };
        let docs = stage("write-docs", StageKind::Agent, Gate::Auto);
        let three = Step::Branch(StageBranch {
            id: "after-plan".into(),
            name: None,
            paths: vec![
                path(
                    "a",
                    &when,
                    vec![implement.clone(), off("testing", StageKind::Testing)],
                ),
                path("b", &when, vec![off("review", StageKind::Review)]),
                path("c", &None, vec![docs.clone()]),
            ],
        });
        assert_eq!(
            active_steps(&[plan.clone(), three, release.clone()]),
            [
                plan.clone(),
                Step::Branch(StageBranch {
                    id: "after-plan".into(),
                    name: None,
                    paths: vec![
                        path("a", &when, vec![implement]),
                        path("c", &None, vec![docs])
                    ],
                }),
                release.clone()
            ]
        );
        let lone = |first: Stage, second: Vec<Stage>| {
            Step::Branch(StageBranch {
                id: "after-plan".into(),
                name: None,
                paths: vec![path("a", &when, vec![first]), path("b", &None, second)],
            })
        };
        let kept = lone(
            off("implement", StageKind::Agent),
            vec![review.clone(), testing.clone()],
        );
        assert_eq!(
            active_steps(&[plan.clone(), kept, release.clone()]),
            [
                plan.clone(),
                Step::Stage(review),
                Step::Stage(testing),
                release.clone()
            ]
        );
        let none = lone(
            off("review", StageKind::Review),
            vec![off("testing", StageKind::Testing)],
        );
        assert_eq!(
            active_steps(&[plan.clone(), none, release.clone()]),
            [plan, release]
        );
    }
}
