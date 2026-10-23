//! The pinned terminals of `packages/core/src/stages/terminal-lane.ts`: the terminal each stage got
//! at its last start, and the stage a terminal works on.

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Stage, Step};
use natamux_proto::stages::terminal_lane::terminal_of_stage;
use serde_json::Value;

/// Stage id to the terminal it got at its last start.
pub type PinnedTerminals = Vec<(String, String)>;

/// From a task's accepted `stage.started` events.
pub fn pinned_terminals_of<'a>(
    events: impl IntoIterator<Item = (bool, &'a TaskEvent)>,
) -> PinnedTerminals {
    let mut pinned: PinnedTerminals = Vec::new();
    for (accepted, event) in events {
        let stage = event.payload.get("stage").and_then(Value::as_str);
        let terminal = event.payload.get("terminal").and_then(Value::as_str);
        if let (true, "stage.started", Some(stage), Some(terminal)) =
            (accepted, event.kind.as_str(), stage, terminal)
        {
            pinned = crate::stages::position::with(&pinned, stage, terminal.to_string());
        }
    }
    pinned
}

/// `terminalOfStage`: a stage keeps its pinned terminal until it starts again.
pub fn terminal_of_stage_pinned(
    steps: &[Step],
    stage_id: &str,
    pinned: &PinnedTerminals,
) -> String {
    crate::stages::position::get(pinned, stage_id)
        .cloned()
        .unwrap_or_else(|| terminal_of_stage(steps, stage_id))
}

/// `stageOfTerminal`: the stage of the step that works in this terminal.
pub fn stage_of_terminal(
    steps: &[Step],
    step: &Step,
    terminal: &str,
    pinned: &PinnedTerminals,
) -> Option<Stage> {
    step.stages()
        .into_iter()
        .find(|stage| terminal_of_stage_pinned(steps, &stage.id, pinned) == terminal)
        .cloned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use natamux_proto::stages::catalog::default_pipeline;
    use natamux_proto::stages::schema::{Gate, StageGroup, StageKind};

    fn pins(list: &[(&str, &str)]) -> PinnedTerminals {
        list.iter()
            .map(|(id, at)| (id.to_string(), at.to_string()))
            .collect()
    }

    fn agent(id: &str) -> Stage {
        Stage::new(id, StageKind::Agent, Gate::Auto)
    }

    fn id_at(
        steps: &[Step],
        step: &Step,
        terminal: &str,
        pinned: &PinnedTerminals,
    ) -> Option<String> {
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
}
