//! Tests of `terminal_lane.rs`: the cases of `packages/core/src/stages/__tests__/terminal-lane.test.ts`
//! for the pinned terminals, `terminalAtStart`, `stageAgent` and `endsShell1`. The lane cases are in
//! `lanes_tests.rs`.

use super::*;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{
    BranchPath, Condition, Gate, StageBranch, StageGroup, StageKind,
};

fn pins(list: &[(&str, &str)]) -> PinnedTerminals {
    list.iter()
        .map(|(id, at)| (id.to_string(), at.to_string()))
        .collect()
}

fn agent(id: &str) -> Stage {
    Stage::new(id, StageKind::Agent, Gate::Auto)
}

fn id_at(steps: &[Step], step: &Step, terminal: &str, pinned: &PinnedTerminals) -> Option<String> {
    stage_of_terminal(steps, step, terminal, pinned).map(|stage| stage.id)
}

#[test]
fn gives_the_stage_of_a_group_by_its_terminal() {
    let build = default_pipeline().remove(3);
    let steps = [build.clone()];
    assert_eq!(
        id_at(&steps, &build, "2", &Vec::new()).as_deref(),
        Some("test-spec")
    );
    assert_eq!(id_at(&steps, &build, "3", &Vec::new()), None);
}

#[test]
fn finds_a_stage_with_its_own_agent_by_its_terminal() {
    let own = |id: &str, output: Option<&str>, agent_name: &str, own_shell: bool| {
        Step::Stage(Stage {
            agent: Some(agent_name.into()),
            output: output.map(Into::into),
            own_shell: own_shell.then_some(true),
            ..agent(id)
        })
    };
    let steps = vec![
        Step::Stage(Stage {
            output: Some("spec.md".into()),
            gate: Gate::Manual,
            ..agent("spec")
        }),
        own("test-spec", Some("test-spec.md"), "copilot", true),
        Step::Stage(agent("implement")),
        own("agent-review", Some("review.md"), "cursor", true),
        own("ci-watch", None, "copilot", false),
    ];
    assert_eq!(
        id_at(&steps, &steps[4], "10", &Vec::new()).as_deref(),
        Some("ci-watch")
    );
    assert_eq!(id_at(&steps, &steps[4], "1", &Vec::new()), None);
    let group = vec![Step::Group(StageGroup {
        id: "build".into(),
        name: None,
        parallel: vec![
            agent("implement"),
            Stage {
                agent: Some("copilot".into()),
                output: Some("test-spec.md".into()),
                ..agent("test-spec")
            },
            Stage {
                output: Some("notes.md".into()),
                ..agent("notes")
            },
            Stage {
                agent: Some("copilot".into()),
                output: Some("lint.md".into()),
                ..agent("lint-notes")
            },
        ],
    })];
    assert_eq!(
        id_at(&group, &group[0], "9", &Vec::new()).as_deref(),
        Some("lint-notes")
    );
}

#[test]
fn finds_a_stage_in_its_own_terminal() {
    let review = Step::Stage(Stage {
        output: Some("review.md".into()),
        own_shell: Some(true),
        ..agent("agent-review")
    });
    let steps = [review.clone()];
    assert_eq!(
        terminal_of_stage_pinned(&steps, "agent-review", &Vec::new()),
        "5"
    );
    assert_eq!(
        id_at(&steps, &review, "5", &Vec::new()).as_deref(),
        Some("agent-review")
    );
    assert_eq!(id_at(&steps, &review, "1", &Vec::new()), None);
}

fn pinned_steps() -> (Vec<Step>, Stage) {
    let test_spec = Stage {
        output: Some("test-spec.md".into()),
        own_shell: Some(true),
        agent: Some("copilot".into()),
        gate: Gate::Manual,
        ..agent("test-spec")
    };
    let spec = Stage {
        output: Some("spec.md".into()),
        gate: Gate::Manual,
        ..agent("spec")
    };
    let steps = vec![
        Step::Stage(spec),
        Step::Stage(test_spec.clone()),
        Step::Stage(agent("implement")),
    ];
    (steps, test_spec)
}

// T-0136: Test spec runs in its own terminal. Then the settings give Spec its own agent.
#[test]
fn keeps_the_stage_of_a_running_agent_when_the_settings_change() {
    let (before, test_spec) = pinned_steps();
    let pinned = pins(&[
        ("spec", "1"),
        ("test-spec", &terminal_of_stage(&before, "test-spec")),
    ]);
    let mut after = before.clone();
    after[0] = Step::Stage(Stage {
        own_shell: Some(true),
        agent: Some("codex".into()),
        ..match &before[0] {
            Step::Stage(spec) => spec.clone(),
            _ => unreachable!(),
        }
    });
    assert_eq!(
        terminal_of_stage_pinned(&after, "test-spec", &pinned),
        pinned[1].1
    );
    let found = id_at(&after, &Step::Stage(test_spec), &pinned[1].1, &pinned);
    assert_eq!(found.as_deref(), Some("test-spec"));
}

#[test]
fn keeps_it_when_the_stage_loses_its_own_agent_while_it_runs() {
    let (before, test_spec) = pinned_steps();
    let pinned = pins(&[
        ("spec", "1"),
        ("test-spec", &terminal_of_stage(&before, "test-spec")),
    ]);
    let lost = vec![
        before[0].clone(),
        Step::Stage(Stage {
            agent: None,
            ..test_spec
        }),
        before[2].clone(),
    ];
    assert_eq!(
        terminal_of_stage_pinned(&lost, "test-spec", &Vec::new()),
        "5"
    );
    assert_eq!(terminal_of_stage_pinned(&lost, "test-spec", &pinned), "7");
}

// `pinnedTerminalsOf` has no case in terminal-lane.test.ts.
#[test]
fn pins_the_terminal_of_each_accepted_stage_started_event() {
    let started = |kind: &str, stage: &str, terminal: &str| {
        let mut event = crate::events::test_fixture::event(kind);
        event.payload.insert("stage".into(), stage.into());
        event.payload.insert("terminal".into(), terminal.into());
        event
    };
    let events = [
        (true, started("stage.started", "spec", "1")),
        (true, started("stage.started", "test-spec", "7")),
        (false, started("stage.started", "implement", "3")),
        (true, started("agent.asked", "plan", "4")),
        (true, started("stage.started", "spec", "5")),
    ];
    let pinned = pinned_terminals_of(events.iter().map(|(accepted, event)| (*accepted, event)));
    assert_eq!(pinned, pins(&[("spec", "5"), ("test-spec", "7")]));
}

fn custom(id: &str, kind: StageKind) -> Stage {
    Stage::new(id, kind, Gate::Auto)
}

fn lane_group(ids: &[&str]) -> Step {
    Step::Group(StageGroup {
        id: "build".into(),
        name: None,
        parallel: ids
            .iter()
            .map(|id| match *id {
                "implement" => agent(id),
                other => Stage {
                    output: Some(format!("{other}.md")),
                    ..agent(other)
                },
            })
            .collect(),
    })
}

fn spec_stage() -> Stage {
    Stage {
        output: Some("spec.md".into()),
        gate: Gate::Manual,
        ..agent("spec")
    }
}

fn after_steps() -> (Vec<Step>, Vec<Step>, Vec<Step>) {
    let (before, test_spec) = pinned_steps();
    let after = vec![
        Step::Stage(Stage {
            own_shell: Some(true),
            agent: Some("codex".into()),
            ..spec_stage()
        }),
        before[1].clone(),
        before[2].clone(),
    ];
    let lost = vec![
        before[0].clone(),
        Step::Stage(Stage {
            agent: None,
            ..test_spec
        }),
        before[2].clone(),
    ];
    (before, after, lost)
}

#[test]
fn keeps_it_at_the_next_start_while_the_stage_needs_the_same_kind_of_shell() {
    let (before, after, _) = after_steps();
    let pinned = pins(&[
        ("spec", "1"),
        ("test-spec", &terminal_of_stage(&before, "test-spec")),
    ]);
    assert_eq!(terminal_at_start(&after, "test-spec", &pinned), "7");
    assert_eq!(terminal_at_start(&after, "implement", &pinned), "1");
}

#[test]
fn takes_the_shell_of_the_settings_at_the_next_start_when_the_stage_gained_or_lost_its_agent() {
    let (before, after, lost) = after_steps();
    let pinned = pins(&[
        ("spec", "1"),
        ("test-spec", &terminal_of_stage(&before, "test-spec")),
    ]);
    assert_eq!(terminal_at_start(&after, "spec", &pinned), "6");
    assert_eq!(terminal_at_start(&lost, "test-spec", &pinned), "5");
}

#[test]
fn moves_a_stage_at_its_next_start_when_its_own_shell_was_turned_on_or_off() {
    let (before, _, _) = after_steps();
    assert_eq!(
        terminal_at_start(&before, "spec", &pins(&[("spec", "5")])),
        "1"
    );
    let own_shell = vec![
        Step::Stage(Stage {
            own_shell: Some(true),
            ..spec_stage()
        }),
        before[1].clone(),
        before[2].clone(),
    ];
    assert_eq!(
        terminal_at_start(&own_shell, "spec", &pins(&[("spec", "1")])),
        "5"
    );
}

#[test]
fn keeps_a_group_lane_at_its_next_start_when_the_lane_number_changed() {
    let group = [lane_group(&["implement", "notes", "review"])];
    assert_eq!(terminal_of_stage(&group, "review"), "3");
    assert_eq!(
        terminal_at_start(&group, "review", &pins(&[("review", "2")])),
        "2"
    );
}

fn own_words() -> StageAgent {
    StageAgent {
        agent: Some("codex".into()),
        args: Some("--model gpt-5".into()),
    }
}

fn with_own(stage: Stage) -> Stage {
    Stage {
        agent: Some("codex".into()),
        args: Some("--model gpt-5".into()),
        ..stage
    }
}

#[test]
fn runs_in_shell_5_apart_from_the_agent_in_shell_1_and_the_stages_of_a_group() {
    let review = Stage {
        output: Some("review.md".into()),
        own_shell: Some(true),
        ..agent("agent-review")
    };
    let codex = Stage {
        agent: Some("codex".into()),
        ..review
    };
    assert_eq!(
        stage_agent(&codex),
        StageAgent {
            agent: Some("codex".into()),
            args: None
        }
    );
}

#[test]
fn gives_them_to_every_agent_or_testing_stage_when_the_task_runs() {
    let test_spec = with_own(Stage {
        output: Some("test-spec.md".into()),
        gate: Gate::Manual,
        ..agent("test-spec")
    });
    assert_eq!(stage_agent(&test_spec), own_words());
    // Implement and Spec run in Terminal 1 without them, in a terminal of their own with them.
    assert_eq!(stage_agent(&with_own(agent("implement"))), own_words());
    let spec = with_own(Stage {
        gate: Gate::Manual,
        ..agent("spec")
    });
    assert_eq!(stage_agent(&spec), own_words());
    let testing = Stage::new("testing", StageKind::Testing, Gate::Auto);
    assert_eq!(stage_agent(&with_own(testing)), own_words());
    assert_eq!(stage_agent(&agent("write-docs")), StageAgent::default());
}

#[test]
fn gives_them_to_no_stage_that_no_agent_works_on() {
    for kind in [StageKind::Review, StageKind::Manual, StageKind::Release] {
        let stage = with_own(Stage::new("my-step", kind, Gate::Manual));
        assert_eq!(stage_agent(&stage), StageAgent::default(), "{kind:?}");
    }
}

fn ends(steps: &[Step], id: &str) -> bool {
    let stage = steps
        .iter()
        .flat_map(|step| step.stages())
        .find(|each| each.id == id)
        .cloned();
    stage.is_some_and(|stage| ends_shell1(steps, &stage, &Vec::new()))
}

#[test]
fn would_end_shell_1_for_the_stages_of_the_default_pipeline_that_run_there() {
    let steps = default_pipeline();
    let ids = [
        "context",
        "spec",
        "plan",
        "implement",
        "test-spec",
        "testing",
        "agent-review",
    ];
    let got: Vec<bool> = ids.iter().map(|id| ends(&steps, id)).collect();
    assert_eq!(got, [true, true, true, true, false, true, false]);
}

#[test]
fn ends_a_shell_of_its_own_for_a_testing_stage_with_a_command_or_a_stage_with_its_own_agent() {
    let steps = vec![
        Step::Stage(Stage {
            command: Some("pnpm test".into()),
            ..custom("testing", StageKind::Testing)
        }),
        Step::Stage(Stage {
            args: Some("--model opus".into()),
            ..custom("write-docs", StageKind::Agent)
        }),
        Step::Stage(custom("notes", StageKind::Agent)),
    ];
    let got: Vec<bool> = ["testing", "write-docs", "notes"]
        .iter()
        .map(|id| ends(&steps, id))
        .collect();
    assert_eq!(got, [false, false, true]);
}

#[test]
fn would_end_shell_1_for_a_stage_in_a_path_of_a_branch() {
    let path = |id: &str, when: Option<Condition>, stage: Stage| BranchPath {
        id: id.into(),
        name: None,
        when,
        not: None,
        stages: vec![stage],
    };
    let steps = vec![Step::Branch(StageBranch {
        id: "after-plan".into(),
        name: None,
        paths: vec![
            path(
                "code",
                Some(Condition::Command { run: "true".into() }),
                agent("implement"),
            ),
            path("text", None, agent("docs")),
        ],
    })];
    assert!(ends(&steps, "implement"));
}
