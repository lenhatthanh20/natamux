//! The agent routes of `routes/agents.ts`: the hooks, the activity and the usage of agents, start
//! an agent by hand, reopen the agent of a finished task, and a review comment for the agent.

use std::sync::Arc;

use hyper::body::Incoming;
use hyper::{HeaderMap, Request, StatusCode};
use natamux_agents::hooks::COPILOT_EVENT_FIELD;
use natamux_agents::launch::is_launchable;
use natamux_agents::usage::limits::claude_limits;
use natamux_core::js::js_trim;
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issues, Rule, ZodError, object, string};
use natamux_proto::agents::claude_code::{TASK_ID_HEADER, TERMINAL_HEADER};
use serde_json::{Value, json};

use crate::agents::errors::AgentError;
use crate::http::agent_rules::Caller;
use crate::http::body::{DEFAULT_BODY_LIMIT, read_body};
use crate::http::errors::{
    Answer, hook_error, internal_error, json_answer, stage_error, zod_answer,
};
use crate::http::routes::stages::stage_answer;
use crate::http::routes::workspaces::branch_field;
use crate::http::server::{AppState, query_value};
use crate::stages::errors::{StageError, StageResult, action};
use crate::terminals::limits::MAIN_TERMINAL;
use crate::workspaces::errors::WorkspaceError;

/// A failed workspace as a stage error: a task error keeps its status, the rest is a 409.
fn workspace_failure(error: WorkspaceError) -> StageError {
    match error {
        WorkspaceError::Task(error) => StageError::Task(error),
        other => action(other.to_string()),
    }
}

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

type StartBody = (String, Option<String>, Option<String>, Option<String>);

/// `startAgentBodySchema`: the agent, and a prompt, a base and a branch, all optional.
fn start_body(value: Option<&Value>) -> Result<StartBody, ZodError> {
    let mut issues = Issues::default();
    let mut parsed = None;
    if let Some(map) = object(value, &[], &mut issues) {
        let agent = string(
            map.get("agent"),
            &[json!("agent")],
            &[Rule::Min(1), Rule::Max(64)],
            &mut issues,
        );
        let prompt = map.get("prompt").map(|prompt| {
            string(
                Some(prompt),
                &[json!("prompt")],
                &[Rule::Max(20_000)],
                &mut issues,
            )
        });
        // The base and the branch of a new workspace, for a task that has none.
        let base = map.get("base").and_then(|base| {
            string(
                Some(base),
                &[json!("base")],
                &[Rule::Min(1), Rule::Max(255)],
                &mut issues,
            )
        });
        let branch = branch_field(map, &mut issues);
        parsed = agent.map(|agent| (agent, prompt.flatten(), base, branch));
    }
    issues.into_result(parsed)
}

/// What the agent of each task does now, from its last hook.
pub fn activity(state: &AppState) -> Answer {
    json_answer(
        StatusCode::OK,
        &json!({ "activity": Value::Object(state.hooks.activity()) }),
    )
}

/// The tokens and estimated cost of each task that has usage.
pub fn usage(state: &AppState) -> Answer {
    json_answer(
        StatusCode::OK,
        &json!({ "usage": Value::Object(state.usage.all()) }),
    )
}

/// The plan limits each agent told last, without windows that have reset.
pub fn limits(state: &AppState) -> Answer {
    json_answer(
        StatusCode::OK,
        &json!({ "limits": to_value(&state.usage.limits()) }),
    )
}

/// The `rate_limits` of Claude Code's status line input, sent by `natamux statusline`. Only
/// Claude Code tells its limits this way; Codex's come from its session file.
pub fn post_limits(state: &AppState, agent: &str, body: Option<Value>) -> Answer {
    if agent != "claude-code" {
        return json_answer(
            StatusCode::NOT_FOUND,
            &json!({ "error": format!("{agent} does not send its limits") }),
        );
    }
    let Some(rate_limits) = body.filter(Value::is_object) else {
        return json_answer(
            StatusCode::BAD_REQUEST,
            &json!({ "error": "The body must be the rate_limits object" }),
        );
    };
    state.usage.set_limits(agent, claude_limits(&rate_limits));
    json_answer(StatusCode::OK, &json!({ "ok": true }))
}

/// `hookParamsSchema` and `hookQuerySchema`: the agent (1 or more characters) and the event of
/// the query (1 to 64 characters), if it has one.
fn hook_inputs(agent: &str, query: &str) -> Result<Option<String>, ZodError> {
    let mut issues = Issues::default();
    string(
        Some(&json!(agent)),
        &[json!("agent")],
        &[Rule::Min(1)],
        &mut issues,
    );
    let event = query_value(query, "event").and_then(|event| {
        string(
            Some(&event),
            &[json!("event")],
            &[Rule::Min(1), Rule::Max(64)],
            &mut issues,
        )
    });
    issues.into_result(Some(event))
}

/// A header, trimmed; None for none or an empty one.
fn header_text(headers: &HeaderMap, name: &str) -> Option<String> {
    let value = headers
        .get(name.to_ascii_lowercase())?
        .to_str()
        .ok()
        .map(js_trim)?;
    (!value.is_empty()).then(|| value.to_string())
}

/// An agent posts its hook here, with its name in the path. Always answer 200 with an empty
/// object, so a hook that matches no task or changes nothing never shows up as an error in the
/// agent. The hook runs on a blocking thread: it reads and writes the task files.
pub async fn post_hook(
    request: Request<Incoming>,
    state: Arc<AppState>,
    who: Caller,
    agent: String,
    query: String,
) -> Answer {
    let (head, body) = request.into_parts();
    let body = match read_body(&head.headers, body, DEFAULT_BODY_LIMIT).await {
        Ok(body) => body,
        Err(error) => return error.answer(),
    };
    let event = match hook_inputs(&agent, &query) {
        Ok(event) => event,
        Err(error) => return zod_answer(&error),
    };
    let task = header_text(&head.headers, TASK_ID_HEADER);
    let terminal = header_text(&head.headers, TERMINAL_HEADER);
    // Copilot CLI does not name the event in the JSON; its hook command adds it as ?event=.
    let body = match (event, body) {
        (Some(event), Some(Value::Object(mut map))) => {
            map.insert(COPILOT_EVENT_FIELD.into(), Value::String(event));
            Value::Object(map)
        }
        (_, body) => body.unwrap_or(Value::Null),
    };
    let hooks = Arc::clone(&state.hooks);
    let handled = tokio::task::spawn_blocking(move || {
        let terminal = terminal.as_deref();
        match &who {
            // The agent token names its task; the header and the folder count for the user only.
            Caller::Agent(own) => hooks.handle_own(&agent, &body, own, terminal),
            _ => hooks.handle(&agent, &body, task.as_deref(), terminal),
        }
    })
    .await;
    match handled {
        Ok(Ok(_)) => json_answer(StatusCode::OK, &json!({})),
        Ok(Err(error)) => hook_error(&error),
        Err(_) => internal_error(),
    }
}

pub async fn start_agent(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let (agent, prompt, base, branch) = match start_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    // Checked before anything is done for nothing.
    if !is_launchable(&agent) {
        return stage_error(&StageError::Agent(AgentError::Unknown(agent)));
    }
    stage_answer(StatusCode::OK, move || {
        let workspaces = Arc::clone(&state.workspaces);
        let make = || {
            workspaces
                .create(&id, base.as_deref(), branch.as_deref())
                .map(|_| ())
                .map_err(workspace_failure)
        };
        let task = state
            .stages
            .run
            .start_by_hand(&id, &agent, prompt.as_deref(), Some(&make))?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

/// The agent of Terminal 1 of a done or cancelled task, on its last session.
pub async fn reopen_agent(state: Arc<AppState>, id: String) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    stage_answer(StatusCode::OK, move || {
        let task = state.stages.run.reopen_agent(&id)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}

/// Puts a prompt in front of the agent of Terminal 1: wakes it when paused, else pastes it.
pub(crate) fn paste_prompt(state: &AppState, id: &str, text: &str) -> StageResult<()> {
    let run = &state.stages.run;
    if !run.wake_paused(id, MAIN_TERMINAL, text)? {
        // Pasted into a shell, the lines of the comment would run as commands.
        if !run.agent_runs(id, MAIN_TERMINAL) {
            return Err(action(format!(
                "No agent runs in the terminal of {id}. Start the agent first"
            )));
        }
        state.terminals.paste(id, text, MAIN_TERMINAL)?;
    }
    Ok(())
}

/// A review comment for the agent: pasted into its terminal as the next prompt.
pub async fn message(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let mut issues = Issues::default();
    let text = object(body.as_ref(), &[], &mut issues).and_then(|map| {
        string(
            map.get("text"),
            &[json!("text")],
            &[Rule::Trim, Rule::Min(1), Rule::Max(20_000)],
            &mut issues,
        )
    });
    let text = match issues.into_result(text) {
        Ok(text) => text,
        Err(error) => return zod_answer(&error),
    };
    stage_answer(StatusCode::OK, move || {
        state.tasks.get_task(&id)?;
        paste_prompt(&state, &id, &text)?;
        let task = state.tasks.record_review_comment(&id, &text)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}
