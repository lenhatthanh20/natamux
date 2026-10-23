//! Helpers of the event tests (`packages/core/src/events/__tests__/events.fixture.ts`).

use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::catalog::default_pipeline;
use natamux_proto::stages::schema::{Gate, Stage, StageBranch, StageKind, Step};
use natamux_proto::tasks::task::TaskState;
use serde_json::{Map, Value, json};

use crate::events::apply::{AppliedEvent, apply_task_event};
use crate::stages::position::{Lanes, Paths, StagePosition};

/// The pipeline the cases were written for: a person reviews, then the tests run. The default
/// pipeline has an agent review after the tests instead.
pub fn pipeline() -> Vec<Step> {
    default_pipeline()
        .into_iter()
        .flat_map(|step| match step.id() {
            "agent-review" => vec![],
            "testing" => vec![
                Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual)),
                step,
            ],
            _ => vec![step],
        })
        .collect()
}

/// `LEGACY_PIPELINE`.
pub fn legacy_pipeline() -> Vec<Step> {
    vec![
        Step::Stage(Stage::new("implement", StageKind::Agent, Gate::Auto)),
        Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual)),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ]
}

pub fn event(kind: &str) -> TaskEvent {
    event_with(kind, json!({}))
}

pub fn event_with(kind: &str, payload: Value) -> TaskEvent {
    TaskEvent {
        task_id: "T-1".into(),
        source: "system".into(),
        kind: kind.into(),
        at: "2026-10-08T09:00:00Z".into(),
        payload: match payload {
            Value::Object(map) => map,
            _ => Map::new(),
        },
    }
}

pub fn lanes(list: &[(&str, TaskState)]) -> Lanes {
    list.iter()
        .map(|(id, state)| (id.to_string(), *state))
        .collect()
}

pub fn paths(list: &[(&str, &str)]) -> Paths {
    list.iter()
        .map(|(id, path)| (id.to_string(), path.to_string()))
        .collect()
}

/// A position with no record of paths.
pub fn at(stage: &str, state: TaskState) -> StagePosition {
    StagePosition::new(stage, state)
}

pub fn at_lanes(stage: &str, state: TaskState, list: &[(&str, TaskState)]) -> StagePosition {
    StagePosition {
        lanes: lanes(list),
        ..at(stage, state)
    }
}

/// The group `build` of the pipeline, with the state of each of its two lanes.
pub fn build(state: TaskState, test_spec: TaskState, implement: TaskState) -> StagePosition {
    at_lanes(
        "build",
        state,
        &[("test-spec", test_spec), ("implement", implement)],
    )
}

pub fn applied(position: StagePosition, accepted: bool, moved: bool) -> AppliedEvent {
    AppliedEvent {
        position,
        accepted,
        moved,
    }
}

fn apply_all(
    steps: &[Step],
    position: StagePosition,
    events: &[TaskEvent],
) -> (StagePosition, Vec<bool>) {
    let mut current = position;
    let mut accepted = Vec::new();
    for next in events {
        let result = apply_task_event(&current, next, steps);
        current = result.position;
        accepted.push(result.accepted);
    }
    (current, accepted)
}

/// Applies the events in order on `pipeline()`.
pub fn run(position: StagePosition, events: &[TaskEvent]) -> (StagePosition, Vec<bool>) {
    apply_all(&pipeline(), position, events)
}

pub fn plan() -> Stage {
    Stage {
        output: Some("plan.md".into()),
        ..Stage::new("plan", StageKind::Agent, Gate::Manual)
    }
}

pub fn branch() -> StageBranch {
    serde_json::from_value(json!({
        "id": "after-plan",
        "paths": [
            {
                "id": "small",
                "when": {"type": "output", "file": "plan.md", "section": "Size", "contains": "small"},
                "stages": [
                    {"id": "implement", "kind": "agent", "gate": "auto"},
                    {"id": "testing", "kind": "testing", "gate": "auto"}
                ]
            },
            {
                "id": "pr-review",
                "when": {"type": "command", "run": "test -f .no-review"},
                "not": true,
                "stages": [{"id": "review", "kind": "review", "gate": "manual"}]
            }
        ]
    }))
    .unwrap()
}

/// `steps`: a plan, a branch with two paths, then the release.
pub fn steps() -> Vec<Step> {
    vec![
        Step::Stage(plan()),
        Step::Branch(branch()),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ]
}

/// `SMALL`.
pub fn small() -> Paths {
    paths(&[("after-plan", "small")])
}

/// A position that keeps the record of the paths taken.
pub fn on(stage: &str, state: TaskState, taken: Paths) -> StagePosition {
    StagePosition {
        paths: Some(taken),
        ..at(stage, state)
    }
}

/// Applies the events in order on `steps()`.
pub fn walk(position: StagePosition, events: &[TaskEvent]) -> (StagePosition, Vec<bool>) {
    apply_all(&steps(), position, events)
}
