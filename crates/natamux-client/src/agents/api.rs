//! The routes of `packages/daemon/src/http/routes/agents.ts` that the app calls.

use std::collections::BTreeMap;

use hyper::Method;
use natamux_proto::agents::activity::{ActivityList, TaskActivity, TaskUsage, UsageList};
use natamux_proto::agents::limits::{AgentLimits, LimitList};
use natamux_proto::agents::start::{AgentMessage, StartAgentInput};
use natamux_proto::tasks::task::{Task, TaskAnswer};
use natamux_proto::terminals::query::encode;

use crate::config::Config;
use crate::error::ClientError;
use crate::http::{call, call_with};

/// What the agents do now, by task id.
pub async fn activity(config: &Config) -> Result<BTreeMap<String, TaskActivity>, ClientError> {
    let list: ActivityList = call(config, Method::GET, "/activity").await?;
    Ok(list.activity)
}

/// What each task used, by task id.
pub async fn usage(config: &Config) -> Result<BTreeMap<String, TaskUsage>, ClientError> {
    let list: UsageList = call(config, Method::GET, "/usage").await?;
    Ok(list.usage)
}

/// The plan limits each agent told last.
pub async fn limits(config: &Config) -> Result<Vec<AgentLimits>, ClientError> {
    let list: LimitList = call(config, Method::GET, "/limits").await?;
    Ok(list.limits)
}

/// Starts an agent in the task's workspace. A task with no workspace gets one first, from
/// `input.base` when it is set.
pub async fn start_agent(
    config: &Config,
    task_id: &str,
    input: &StartAgentInput,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/agent", encode(task_id));
    let answer: TaskAnswer = call_with(config, Method::POST, &path, input).await?;
    Ok(answer.task)
}

/// Starts the agent of a finished task again, on its session.
pub async fn reopen_agent(config: &Config, task_id: &str) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/agent/reopen", encode(task_id));
    let answer: TaskAnswer = call(config, Method::POST, &path).await?;
    Ok(answer.task)
}

/// Types text into the task's running agent.
pub async fn send_to_agent(
    config: &Config,
    task_id: &str,
    message: &AgentMessage,
) -> Result<Task, ClientError> {
    let path = format!("/tasks/{}/agent/message", encode(task_id));
    let answer: TaskAnswer = call_with(config, Method::POST, &path, message).await?;
    Ok(answer.task)
}
