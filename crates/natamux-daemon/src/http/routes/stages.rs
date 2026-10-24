//! The stage routes (`routes/stages.ts`): stage actions, the workflow, CI and skip marks, and the
//! stage files. The stage engine waits for processes, so each call runs on a blocking thread.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issues, Pattern, Rule, ZodError, invalid_type, object, one_of, string};
use natamux_stages::actions::{AGENT_REFUSED_ACTIONS, STAGE_ACTIONS, StageAction};
use natamux_stages::files::{list_stage_files, read_stage_file, write_stage_file};
use serde_json::{Value, json};

use crate::http::errors::{
    Answer, error_answer, forbidden, internal_error, json_answer, stage_error, zod_answer,
};
use crate::http::server::AppState;
use crate::stages::act::StageActionInput;
use crate::stages::errors::{StageError, StageResult};

/// Runs stage work on a blocking thread and answers with its JSON, or its error.
pub async fn stage_answer(
    status: StatusCode,
    work: impl FnOnce() -> StageResult<Value> + Send + 'static,
) -> Answer {
    match tokio::task::spawn_blocking(work).await {
        Ok(Ok(body)) => json_answer(status, &body),
        Ok(Err(error)) => stage_error(&error),
        Err(_) => internal_error(),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

fn is_terminal_name(text: &str) -> bool {
    let bytes = text.as_bytes();
    (1..=3).contains(&bytes.len())
        && (b'1'..=b'9').contains(&bytes[0])
        && bytes[1..].iter().all(u8::is_ascii_digit)
}

const TERMINAL: Pattern = Pattern {
    test: is_terminal_name,
    source: "^[1-9]\\d{0,2}$",
    message: None,
};

fn optional_string(
    map: &serde_json::Map<String, Value>,
    key: &str,
    rules: &[Rule],
    issues: &mut Issues,
) -> Option<String> {
    map.get(key)
        .and_then(|value| string(Some(value), &[json!(key)], rules, issues))
}

/// `stageBodySchema`: the action and what it names.
fn stage_body(value: Option<&Value>) -> Result<(StageAction, StageActionInput), ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        let names: Vec<&str> = STAGE_ACTIONS.iter().map(|each| each.as_str()).collect();
        let what = one_of(map.get("action"), &[json!("action")], &names, &mut issues);
        let name = [Rule::Min(1), Rule::Max(40)];
        let input = StageActionInput {
            stage: optional_string(map, "stage", &name, &mut issues),
            terminal: optional_string(map, "terminal", &[Rule::Regex(TERMINAL)], &mut issues),
            to: optional_string(map, "to", &name, &mut issues),
            reason: optional_string(
                map,
                "reason",
                &[Rule::Trim, Rule::Min(1), Rule::Max(20_000)],
                &mut issues,
            ),
            by_agent: false,
        };
        parsed = what
            .and_then(|what| StageAction::parse(&what))
            .map(|what| (what, input));
    }
    issues.into_result(parsed)
}

/// `{ stage, on }` of the workflow, CI and skip marks.
fn mark_body(value: Option<&Value>) -> Result<(String, bool), ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        let stage = string(
            map.get("stage"),
            &[json!("stage")],
            &[Rule::Min(1), Rule::Max(40)],
            &mut issues,
        );
        let on = match map.get("on") {
            Some(Value::Bool(on)) => Some(*on),
            other => {
                issues.push(invalid_type("boolean", other, &[json!("on")]));
                None
            }
        };
        parsed = stage.zip(on);
    }
    issues.into_result(parsed)
}

/// `stageFileParamsSchema`: the task id and a name of 1 to 100 characters.
fn file_params(id: &str, name: &str) -> Result<(String, String), ZodError> {
    let id = task_id_param(id)?;
    let mut issues = Issues::default();
    let name = string(
        Some(&json!(name)),
        &[json!("name")],
        &[Rule::Min(1), Rule::Max(100)],
        &mut issues,
    );
    issues.into_result(name).map(|name| (id, name))
}

/// Moves a task through its stages: approve, send back, skip, pass, fail, report done, start.
pub async fn act(state: Arc<AppState>, id: String, body: Option<Value>, by_agent: bool) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let (what, mut input) = match stage_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    // What only a person may do, so an agent cannot pass a manual gate by itself.
    if by_agent && AGENT_REFUSED_ACTIONS.contains(&what) {
        return forbidden(&format!("{} a stage", what.as_str()));
    }
    input.by_agent = by_agent;
    stage_answer(StatusCode::OK, move || {
        let task = state.stages.run.act(&id, what, &input)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

/// Which mark a `PUT` changes, and what the agent token may not do.
#[derive(Clone, Copy)]
pub enum Mark {
    Workflow,
    Ci,
    Skip,
}

impl Mark {
    fn refused(self) -> &'static str {
        match self {
            Mark::Workflow => "turn a workflow on or off",
            Mark::Ci => "turn a CI check on or off",
            Mark::Skip => "mark a stage to skip",
        }
    }
}

/// Whether a stage runs as a workflow, waits for CI, or is skipped when the task reaches it.
pub async fn set_mark(
    state: Arc<AppState>,
    id: String,
    body: Option<Value>,
    by_agent: bool,
    mark: Mark,
) -> Answer {
    if by_agent {
        return forbidden(mark.refused());
    }
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let (stage, on) = match mark_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    stage_answer(StatusCode::OK, move || {
        let tasks = &state.tasks;
        let task = match mark {
            Mark::Workflow => tasks.set_workflow(&id, &stage, on)?,
            Mark::Ci => tasks.set_ci(&id, &stage, on)?,
            Mark::Skip => tasks.set_skip(&id, &stage, on)?,
        };
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

pub async fn list_files(state: Arc<AppState>, id: String) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    stage_answer(StatusCode::OK, move || {
        let task = state.tasks.get_task(&id)?;
        let files = list_stage_files(&state.data_dir, &task.id, task.workspace.as_deref());
        let files: Vec<Value> = files
            .iter()
            .map(|file| json!({ "name": file.name, "size": file.size, "modifiedAt": file.modified_at }))
            .collect();
        Ok(json!({ "files": files }))
    })
    .await
}

pub async fn read_file(state: Arc<AppState>, id: String, name: String) -> Answer {
    let (id, name) = match file_params(&id, &name) {
        Ok(params) => params,
        Err(error) => return zod_answer(&error),
    };
    let missing = format!("{id} has no {name}");
    let read = tokio::task::spawn_blocking(move || -> StageResult<Option<Value>> {
        let task = state.tasks.get_task(&id)?;
        let text = read_stage_file(&state.data_dir, &task.id, task.workspace.as_deref(), &name)
            .map_err(|error| StageError::File(error.to_string()))?;
        Ok(text.map(|text| json!({ "name": name, "text": text })))
    })
    .await;
    match read {
        Ok(Ok(Some(body))) => json_answer(StatusCode::OK, &body),
        Ok(Ok(None)) => error_answer(StatusCode::NOT_FOUND, "not_found", &missing),
        Ok(Err(error)) => stage_error(&error),
        Err(_) => internal_error(),
    }
}

/// An edit by hand, for example to fix the spec before approving it.
pub async fn write_file(
    state: Arc<AppState>,
    id: String,
    name: String,
    body: Option<Value>,
) -> Answer {
    let (id, name) = match file_params(&id, &name) {
        Ok(params) => params,
        Err(error) => return zod_answer(&error),
    };
    let mut issues = Issues::default();
    let text = object(body.as_ref(), &[], &mut issues)
        .and_then(|map| string(map.get("text"), &[json!("text")], &[], &mut issues));
    let text = match issues.into_result(text) {
        Ok(text) => text,
        Err(error) => return zod_answer(&error),
    };
    stage_answer(StatusCode::OK, move || {
        let task = state.tasks.get_task(&id)?;
        write_stage_file(
            &state.data_dir,
            &task.id,
            task.workspace.as_deref(),
            &name,
            &text,
        )
        .map_err(|error| StageError::File(error.to_string()))?;
        Ok(json!({ "name": name, "text": text }))
    })
    .await
}
