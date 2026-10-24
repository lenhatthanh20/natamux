//! The agent-start routes of `routes/agents.ts`: start an agent by hand, reopen the agent of a
//! finished task, and a review comment for the agent. The hook, activity and usage routes come
//! with part 4.

use std::sync::Arc;

use hyper::StatusCode;
use natamux_agents::launch::is_launchable;
use natamux_core::tasks::inputs::task_id_param;
use natamux_core::zod::{Issues, Rule, ZodError, object, string};
use serde_json::{Value, json};

use crate::agents::errors::AgentError;
use crate::http::errors::{Answer, stage_error, zod_answer};
use crate::http::routes::stages::stage_answer;
use crate::http::server::AppState;
use crate::stages::errors::{StageError, action};
use crate::terminals::limits::MAIN_TERMINAL;

fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(Value::Null)
}

/// `startAgentBodySchema`: the agent, and a prompt and a base branch, both optional.
fn start_body(value: Option<&Value>) -> Result<(String, Option<String>), ZodError> {
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
        // The base branch of a new workspace: checked, not used before part 5.
        if let Some(base) = map.get("base") {
            string(
                Some(base),
                &[json!("base")],
                &[Rule::Min(1), Rule::Max(255)],
                &mut issues,
            );
        }
        parsed = agent.map(|agent| (agent, prompt.flatten()));
    }
    issues.into_result(parsed)
}

pub async fn start_agent(state: Arc<AppState>, id: String, body: Option<Value>) -> Answer {
    let id = match task_id_param(&id) {
        Ok(id) => id,
        Err(error) => return zod_answer(&error),
    };
    let (agent, prompt) = match start_body(body.as_ref()) {
        Ok(parsed) => parsed,
        Err(error) => return zod_answer(&error),
    };
    // Checked before anything is done for nothing.
    if !is_launchable(&agent) {
        return stage_error(&StageError::Agent(AgentError::Unknown(agent)));
    }
    stage_answer(StatusCode::OK, move || {
        let task = state
            .stages
            .run
            .start_by_hand(&id, &agent, prompt.as_deref())?;
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
        // Pasted into a shell, the lines of the comment would run as commands.
        if !state.stages.run.agent_runs(&id, MAIN_TERMINAL) {
            return Err(action(format!(
                "No agent runs in the terminal of {id}. Start the agent first"
            )));
        }
        state.terminals.paste(&id, &text, MAIN_TERMINAL)?;
        let task = state.tasks.record_review_comment(&id, &text)?;
        Ok(json!({ "task": to_value(&task) }))
    })
    .await
}
