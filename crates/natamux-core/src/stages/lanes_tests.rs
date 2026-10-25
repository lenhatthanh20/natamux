//! Ports the `terminalLane` cases of `packages/core/src/stages/__tests__/terminal-lane.test.ts`:
//! the 8 of 'terminalLane' and the lane checks inside 'a stage with its own agent', 'a stage in its
//! own shell', 'a branch' and 'a terminal pinned at the start of its stage'.

use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{BranchPath, Condition, Gate, StageBranch, StageKind};
use natamux_proto::tasks::task::TaskState::{self, Done, NeedsInput, Review, Running};

use super::*;
use crate::events::test_fixture::{at, at_lanes};
use crate::stages::position::combined_state;
use crate::stages::terminal_lane::PinnedTerminals;
use natamux_proto::stages::schema::Stage;

fn none() -> PinnedTerminals {
    Vec::new()
}

fn expect(got: TerminalLane, stage: Option<&str>, state: Option<TaskState>) {
    assert_eq!(got.stage.as_deref(), stage);
    assert_eq!(got.state, state);
    assert_eq!(got.agent, None);
}

fn is(got: TerminalLane, stage: &str, state: TaskState) {
    expect(got, Some(stage), Some(state));
}

fn nothing(got: TerminalLane) {
    expect(got, None, None);
}

fn agent(id: &str) -> Stage {
    Stage::new(id, StageKind::Agent, Gate::Auto)
}

fn writing(id: &str, output: &str) -> Stage {
    Stage {
        output: Some(output.into()),
        ..agent(id)
    }
}

fn own(stage: Stage, name: &str) -> Stage {
    Stage {
        agent: Some(name.into()),
        ..stage
    }
}

fn own_shell(stage: Stage) -> Stage {
    Stage {
        own_shell: Some(true),
        ..stage
    }
}

fn one(stage: Stage) -> Step {
    Step::Stage(stage)
}

fn own_agent_steps() -> Vec<Step> {
    vec![
        one(Stage {
            gate: Gate::Manual,
            ..writing("spec", "spec.md")
        }),
        one(Stage {
            gate: Gate::Manual,
            ..own(own_shell(writing("test-spec", "test-spec.md")), "copilot")
        }),
        one(agent("implement")),
        one(own(
            own_shell(writing("agent-review", "review.md")),
            "cursor",
        )),
        one(own(agent("ci-watch"), "copilot")),
        one(own(agent("merge-pr"), "copilot")),
    ]
}

#[test]
fn gets_shell_6_plus_its_index_among_all_stages_and_shell_1_keeps_the_rest() {
    let steps = own_agent_steps();
    let position = at("ci-watch", Running);
    is(
        terminal_lane(&position, &steps, "10", &none()),
        "ci-watch",
        Running,
    );
    // Shell 1 served Spec and Implement; the last of them names it.
    is(
        terminal_lane(&position, &steps, "1", &none()),
        "implement",
        Done,
    );
}

#[test]
fn keeps_the_stage_of_a_shell_after_its_step_is_over() {
    let steps = own_agent_steps();
    let at_implement = at("implement", Running);
    is(
        terminal_lane(&at_implement, &steps, "7", &none()),
        "test-spec",
        Done,
    );
    // A stage that has not run yet does not name a shell.
    nothing(terminal_lane(&at_implement, &steps, "9", &none()));
    nothing(terminal_lane(&at_implement, &steps, "2", &none()));
    nothing(terminal_lane(&at("spec", Running), &steps, "7", &none()));
    is(
        terminal_lane(&at("merge-pr", Done), &steps, "7", &none()),
        "test-spec",
        Done,
    );
}

#[test]
fn runs_in_shell_5_apart_from_the_agent_in_shell_1_and_the_stages_of_a_group() {
    let review = own_shell(writing("agent-review", "review.md"));
    let position = at("agent-review", Running);
    let steps = [one(review)];
    is(
        terminal_lane(&position, &steps, "5", &none()),
        "agent-review",
        Running,
    );
    nothing(terminal_lane(&position, &steps, "1", &none()));
}

fn build() -> Step {
    default_pipeline().remove(3)
}

fn in_build(lanes: &[(&str, TaskState)]) -> StagePosition {
    let list: Vec<(String, TaskState)> = lanes.iter().map(|(id, s)| (id.to_string(), *s)).collect();
    StagePosition {
        state: combined_state(&list),
        ..at_lanes("build", Running, lanes)
    }
}

#[test]
fn gives_each_terminal_of_a_group_the_state_of_its_own_lane() {
    let steps = [build()];
    let position = in_build(&[("implement", Running), ("test-spec", Review)]);
    assert_eq!(position.state, Review);
    is(
        terminal_lane(&position, &steps, "1", &none()),
        "implement",
        Running,
    );
    is(
        terminal_lane(&position, &steps, "2", &none()),
        "test-spec",
        Review,
    );
}

#[test]
fn keeps_the_lanes_apart_when_one_needs_input() {
    let steps = [build()];
    let position = in_build(&[("implement", Running), ("test-spec", NeedsInput)]);
    assert_eq!(position.state, NeedsInput);
    assert_eq!(
        terminal_lane(&position, &steps, "1", &none()).state,
        Some(Running)
    );
    assert_eq!(
        terminal_lane(&position, &steps, "2", &none()).state,
        Some(NeedsInput)
    );
}

#[test]
fn shows_a_finished_lane_as_done_also_when_it_is_gone_from_the_lanes() {
    let steps = [build()];
    let finished = in_build(&[("implement", Running), ("test-spec", Done)]);
    is(
        terminal_lane(&finished, &steps, "2", &none()),
        "test-spec",
        Done,
    );
    let gone = StagePosition::new("build", Running);
    is(
        terminal_lane(&gone, &steps, "2", &none()),
        "test-spec",
        Done,
    );
}

#[test]
fn gives_no_lane_to_a_terminal_the_group_does_not_use() {
    let position = in_build(&[("implement", Running), ("test-spec", Review)]);
    nothing(terminal_lane(&position, &[build()], "3", &none()));
}

#[test]
fn shows_the_shell_of_a_stage_before_the_group_as_done() {
    let spec = one(Stage {
        gate: Gate::Manual,
        ..own_shell(writing("spec", "spec.md"))
    });
    let position = in_build(&[("implement", Running), ("test-spec", Review)]);
    is(
        terminal_lane(&position, &[spec, build()], "5", &none()),
        "spec",
        Done,
    );
}

#[test]
fn shows_the_shell_of_a_group_stage_as_done_once_the_task_is_past_the_group() {
    let steps = default_pipeline();
    let position = at("testing", Running);
    is(
        terminal_lane(&position, &steps, "2", &none()),
        "test-spec",
        Done,
    );
    // Shell 1 is the shell of the current stage too.
    is(
        terminal_lane(&position, &steps, "1", &none()),
        "testing",
        Running,
    );
}

#[test]
fn gives_the_task_state_to_shell_1_outside_a_group_and_no_lane_to_the_others() {
    let spec = steps_spec();
    let position = at("spec", Running);
    is(
        terminal_lane(&position, &spec, "1", &none()),
        "spec",
        Running,
    );
    nothing(terminal_lane(&position, &spec, "2", &none()));
}

fn steps_spec() -> Vec<Step> {
    vec![default_pipeline().remove(1)]
}

#[test]
fn uses_the_stage_of_the_task_when_its_step_is_unknown() {
    let position = at("gone", Review);
    is(terminal_lane(&position, &[], "1", &none()), "gone", Review);
    nothing(terminal_lane(&position, &[], "2", &none()));
}

fn path(id: &str, when: Option<Condition>, stages: Vec<Stage>) -> BranchPath {
    BranchPath {
        id: id.into(),
        name: None,
        when,
        not: None,
        stages,
    }
}

#[test]
fn gives_a_stage_in_a_path_the_shell_of_a_single_stage() {
    let plan = Stage {
        gate: Gate::Manual,
        ..writing("plan", "plan.md")
    };
    let when = Condition::Output {
        file: "plan.md".into(),
        section: Some("Size".into()),
        contains: "small".into(),
    };
    let review = own_shell(writing("agent-review", "review.md"));
    let docs = own(agent("write-docs"), "codex");
    let notes = own(own_shell(writing("notes", "notes.md")), "cursor");
    let shells = vec![
        one(plan),
        Step::Branch(StageBranch {
            id: "after-plan".into(),
            name: None,
            paths: vec![
                path("a", Some(when), vec![agent("implement"), review]),
                path("b", None, vec![docs, notes]),
            ],
        }),
    ];
    let taken = |id: &str| -> Option<crate::stages::position::Paths> {
        Some(vec![("after-plan".to_string(), id.to_string())])
    };
    let at_review = StagePosition {
        paths: taken("a"),
        ..at("agent-review", Running)
    };
    is(
        terminal_lane(&at_review, &shells, "5", &none()),
        "agent-review",
        Running,
    );
    is(
        terminal_lane(&at_review, &shells, "1", &none()),
        "implement",
        Done,
    );
    let at_notes = StagePosition {
        paths: taken("b"),
        ..at("notes", Running)
    };
    is(
        terminal_lane(&at_notes, &shells, "10", &none()),
        "notes",
        Running,
    );
    // Implement is on the path the task did not take.
    is(
        terminal_lane(&at_notes, &shells, "1", &none()),
        "plan",
        Done,
    );
    is(
        terminal_lane(&at_notes, &shells, "9", &none()),
        "write-docs",
        Done,
    );
}

#[test]
fn keeps_the_stage_of_a_running_agent_when_the_settings_change() {
    let test_spec = Stage {
        gate: Gate::Manual,
        ..own(own_shell(writing("test-spec", "test-spec.md")), "copilot")
    };
    let spec = Stage {
        gate: Gate::Manual,
        ..writing("spec", "spec.md")
    };
    let before = vec![
        one(spec.clone()),
        one(test_spec.clone()),
        one(agent("implement")),
    ];
    let after = vec![
        one(own(own_shell(spec), "codex")),
        one(test_spec),
        one(agent("implement")),
    ];
    let pinned: PinnedTerminals = vec![
        ("spec".into(), "1".into()),
        (
            "test-spec".into(),
            natamux_proto::stages::terminal_lane::terminal_of_stage(&before, "test-spec"),
        ),
    ];
    let position = at("test-spec", Running);
    is(
        terminal_lane(&position, &after, &pinned[1].1, &pinned),
        "test-spec",
        Running,
    );
    // Spec ran in Shell 1, so Shell 1 still names it.
    is(terminal_lane(&position, &after, "1", &pinned), "spec", Done);
}
