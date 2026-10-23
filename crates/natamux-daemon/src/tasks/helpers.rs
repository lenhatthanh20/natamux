//! Small rules of the task service (`task-helpers.ts`).

use natamux_core::json::js_keys;
use natamux_core::stages::position::StagePosition;
use natamux_proto::events::task_event::TaskEvent;
use natamux_proto::stages::schema::{Gate, Stage, StageKind, Step};
use natamux_proto::tasks::task::{Task, TaskState};
use serde_json::{Map, Value};

/// How often a running task's sign of life is written down at most.
pub const ACTIVITY_INTERVAL_MS: i64 = 30_000;

/// How often a sign of life alone rewrites the task file at most. The index gets every one.
pub const ACTIVITY_FILE_INTERVAL_MS: i64 = 60_000;

/// The flow tasks had before stages: implement, review, release (`LEGACY_PIPELINE`). The daemon
/// passes the settings of each repository instead.
pub fn legacy_pipeline() -> Vec<Step> {
    vec![
        Step::Stage(Stage::new("implement", StageKind::Agent, Gate::Auto)),
        Step::Stage(Stage::new("review", StageKind::Review, Gate::Manual)),
        Step::Stage(Stage::new("release", StageKind::Release, Gate::Manual)),
    ]
}

/// Compares as SQLite compares text (UTF-8 bytes), so the order stays the one the index gave.
/// shortcut: JS compares UTF-16 units; the two differ only past U+FFFF, which ids and times
/// never hold.
pub fn compare_text(a: &str, b: &str) -> std::cmp::Ordering {
    a.cmp(b)
}

fn state_of(value: &Value) -> TaskState {
    serde_json::from_value(value.clone()).unwrap_or_default()
}

pub fn position_of(task: &Task) -> StagePosition {
    StagePosition {
        stage: task.stage.clone(),
        state: task.state,
        lanes: js_keys(&task.lanes)
            .into_iter()
            .map(|key| (key.clone(), state_of(&task.lanes[key.as_str()])))
            .collect(),
        paths: Some(
            js_keys(&task.paths)
                .into_iter()
                .map(|key| {
                    let path = task.paths[key.as_str()].as_str().unwrap_or("").to_string();
                    (key.clone(), path)
                })
                .collect(),
        ),
    }
}

/// The task at its new position. Entering a new step starts its clock.
pub fn move_task(task: &Task, position: &StagePosition, moved: bool, at: &str) -> Task {
    let lanes: Map<String, Value> = position
        .lanes
        .iter()
        .map(|(id, state)| (id.clone(), Value::String(state.name().into())))
        .collect();
    let paths = match &position.paths {
        Some(paths) => paths
            .iter()
            .map(|(id, path)| (id.clone(), Value::String(path.clone())))
            .collect(),
        None => task.paths.clone(),
    };
    Task {
        stage: position.stage.clone(),
        state: position.state,
        lanes,
        paths,
        stage_since: if moved {
            Some(at.to_string())
        } else {
            task.stage_since.clone()
        },
        ..task.clone()
    }
}

/// An agent waits for its background work from its first `agent.waiting` until it shows up again,
/// or the task leaves running. Other events, such as checks that pass, do not end the wait.
pub fn background_since_after(task: &Task, event: &TaskEvent, state: TaskState) -> Option<String> {
    if event.kind == "agent.waiting" {
        return task
            .background_since
            .clone()
            .or_else(|| Some(event.at.clone()));
    }
    if state != TaskState::Running || event.source == "agent" {
        None
    } else {
        task.background_since.clone()
    }
}
