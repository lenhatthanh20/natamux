use super::*;
use crate::stages::catalog::default_pipeline;
use crate::stages::pipeline_rules::pipeline_issues;
use crate::stages::schema::{BranchPath, Condition, Gate, StageBranch, StageGroup};

fn agent(id: &str, gate: Gate) -> Stage {
    Stage::new(id, StageKind::Agent, gate)
}

fn step(stage: &Stage) -> Step {
    Step::Stage(stage.clone())
}

fn group(id: &str, parallel: Vec<Stage>) -> Step {
    Step::Group(StageGroup {
        id: id.into(),
        name: None,
        parallel,
    })
}

fn shells(steps: &[Step], ids: &[&str]) -> Vec<String> {
    ids.iter().map(|id| terminal_of_stage(steps, id)).collect()
}

fn writing(id: &str, gate: Gate, output: &str) -> Stage {
    Stage {
        output: Some(output.into()),
        ..agent(id, gate)
    }
}

fn own(id: &str) -> Stage {
    Stage {
        agent: Some("copilot".into()),
        ..agent(id, Gate::Auto)
    }
}

fn own_shell(stage: Stage) -> Stage {
    Stage {
        own_shell: Some(true),
        ..stage
    }
}

#[test]
fn gives_the_code_stage_the_first_terminal_and_the_others_the_next_ones() {
    let build = default_pipeline().remove(3);
    let steps = [build];
    assert_eq!(terminal_of_stage(&steps, "implement"), "1");
    assert_eq!(terminal_of_stage(&steps, "test-spec"), "2");
}

// Steps of the issue: the task's agent does Spec and Implement in Terminal 1.
fn issue_steps() -> Vec<Step> {
    vec![
        step(&writing("spec", Gate::Manual, "spec.md")),
        step(&Stage {
            gate: Gate::Manual,
            ..own_shell(Stage {
                output: Some("test-spec.md".into()),
                ..own("test-spec")
            })
        }),
        step(&agent("implement", Gate::Auto)),
        step(&own_shell(Stage {
            agent: Some("cursor".into()),
            output: Some("review.md".into()),
            ..own("agent-review")
        })),
        step(&own("ci-watch")),
        step(&own("merge-pr")),
    ]
}

#[test]
fn gets_shell_6_plus_its_index_among_all_stages_and_shell_1_keeps_the_rest() {
    let steps = issue_steps();
    let ids: Vec<&str> = steps.iter().map(Step::id).collect();
    assert_eq!(shells(&steps, &ids), ["1", "7", "1", "9", "10", "11"]);
}

#[test]
fn gets_one_for_arguments_alone_and_keeps_shell_1_or_5_without_either() {
    let plain = [
        step(&Stage {
            args: Some("--model opus".into()),
            ..agent("write-docs", Gate::Auto)
        }),
        step(&agent("notes", Gate::Auto)),
        step(&own_shell(writing("agent-review", Gate::Auto, "r.md"))),
    ];
    assert_eq!(
        shells(&plain, &["write-docs", "notes", "agent-review"]),
        ["6", "1", "5"]
    );
}

#[test]
fn gives_a_lane_of_a_group_its_own_shell_and_the_other_lanes_keep_theirs() {
    let steps = [group(
        "build",
        vec![
            agent("implement", Gate::Auto),
            Stage {
                output: Some("test-spec.md".into()),
                ..own("test-spec")
            },
            writing("notes", Gate::Auto, "notes.md"),
            Stage {
                output: Some("lint.md".into()),
                ..own("lint-notes")
            },
        ],
    )];
    let ids = ["implement", "test-spec", "notes", "lint-notes"];
    assert_eq!(shells(&steps, &ids), ["1", "7", "3", "9"]);
}

#[test]
fn never_gives_two_stages_the_same_shell_whatever_the_settings_say() {
    let spec = Stage {
        output: Some("spec.md".into()),
        ..own("spec")
    };
    let test_spec = Stage {
        output: Some("test-spec.md".into()),
        ..own("test-spec")
    };
    let all = vec![
        step(&own_shell(spec.clone())),
        step(&own_shell(test_spec)),
        group(
            "build",
            vec![
                own("implement"),
                Stage {
                    output: Some("notes.md".into()),
                    ..own("notes")
                },
            ],
        ),
    ];
    let ids = ["spec", "test-spec", "implement", "notes"];
    assert_eq!(shells(&all, &ids), ["6", "7", "1", "9"]);
    // A stage turned off in the settings still counts, so the numbers after it stay.
    let mut off = all.clone();
    off[0] = step(&Stage {
        enabled: Some(false),
        ..own_shell(spec)
    });
    assert_eq!(terminal_of_stage(&off, "test-spec"), "7");
}

#[test]
fn gives_no_shell_to_an_agent_the_stage_may_not_have() {
    let invalid = [step(&Stage {
        agent: Some("codex".into()),
        ..agent("spec", Gate::Manual)
    })];
    assert_eq!(terminal_of_stage(&invalid, "spec"), "1");
}

#[test]
fn runs_in_shell_5_apart_from_the_agent_in_shell_1_and_the_stages_of_a_group() {
    let review = own_shell(writing("agent-review", Gate::Auto, "review.md"));
    assert_eq!(terminal_of_stage(&[step(&review)], "agent-review"), "5");
    assert_eq!(OWN_SHELL, "5");
}

#[test]
fn gives_a_stage_in_a_path_the_shell_of_a_single_stage() {
    let plan = writing("plan", Gate::Manual, "plan.md");
    let when = Condition::Output {
        file: "plan.md".into(),
        section: Some("Size".into()),
        contains: "small".into(),
    };
    let review = own_shell(writing("agent-review", Gate::Auto, "review.md"));
    let docs = Stage {
        agent: Some("codex".into()),
        ..agent("write-docs", Gate::Auto)
    };
    let notes = own_shell(Stage {
        agent: Some("cursor".into()),
        ..writing("notes", Gate::Auto, "notes.md")
    });
    let path = |id: &str, when: Option<Condition>, stages: Vec<Stage>| BranchPath {
        id: id.into(),
        name: None,
        when,
        not: None,
        stages,
    };
    let steps = [
        step(&plan),
        Step::Branch(StageBranch {
            id: "after-plan".into(),
            name: None,
            paths: vec![
                path(
                    "a",
                    Some(when),
                    vec![agent("implement", Gate::Auto), review],
                ),
                path("b", None, vec![docs, notes]),
            ],
        }),
    ];
    let ids = ["plan", "implement", "agent-review", "write-docs", "notes"];
    assert_eq!(shells(&steps, &ids), ["1", "1", "5", "9", "10"]);
    assert!(pipeline_issues(&steps).is_empty());
}

#[test]
fn lets_a_stage_of_your_own_that_an_agent_works_on_have_its_own_agent_anywhere() {
    let docs = agent("write-docs", Gate::Auto);
    assert!(can_have_own_agent(&step(&docs), &docs));
    let lint = Stage::new("check-types", StageKind::Testing, Gate::Auto);
    assert!(can_have_own_agent(&step(&lint), &lint));
    let manual = Stage::new("my-step", StageKind::Manual, Gate::Manual);
    assert!(!can_have_own_agent(&step(&manual), &manual));
    let spec = agent("spec", Gate::Manual);
    assert!(!can_have_own_agent(&step(&spec), &spec));
    assert!(can_have_own_agent(
        &step(&own_shell(spec.clone())),
        &own_shell(spec)
    ));
}
