//! The errors of an agent start (`TaskWaitingError` of `agent-runner.ts`) and of a hook
//! (`UnknownAgentError` of `agent-hook-service.ts`).

use std::fmt;

use natamux_agents::hooks::known_agent_ids;
use natamux_core::zod::ZodError;
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;

use crate::tasks::errors::TaskError;
use crate::terminals::types::TerminalError;

#[derive(Debug)]
pub enum AgentError {
    /// 404 `unknown_agent`.
    Unknown(String),
    /// 409: the task waits for the tasks it depends on.
    Waiting {
        task_id: String,
        waiting: Vec<String>,
    },
    Task(TaskError),
    Terminal(TerminalError),
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AgentError::Unknown(agent) => write!(
                f,
                "Unknown agent \"{agent}\". Known agents: {}",
                LAUNCHABLE_AGENTS.join(", ")
            ),
            AgentError::Waiting { task_id, waiting } => {
                write!(f, "{task_id} waits for {} to be done", waiting.join(", "))
            }
            AgentError::Task(error) => write!(f, "{error}"),
            AgentError::Terminal(error) => write!(f, "{error}"),
        }
    }
}

impl From<TaskError> for AgentError {
    fn from(error: TaskError) -> Self {
        AgentError::Task(error)
    }
}

impl From<TerminalError> for AgentError {
    fn from(error: TerminalError) -> Self {
        AgentError::Terminal(error)
    }
}

/// What `POST /hooks/:agent` can answer besides 200.
#[derive(Debug)]
pub enum HookError {
    /// 404 `unknown_agent`: no adapter for this name.
    UnknownAgent(String),
    /// 400: the body is not a hook of that agent.
    Invalid(ZodError),
    Task(TaskError),
    /// 500: the stage lookup failed.
    Failed(String),
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HookError::UnknownAgent(agent) => write!(
                f,
                "Unknown agent \"{agent}\". Known agents: {}",
                known_agent_ids().join(", ")
            ),
            HookError::Invalid(error) => write!(f, "{}", error.describe()),
            HookError::Task(error) => write!(f, "{error}"),
            HookError::Failed(message) => f.write_str(message),
        }
    }
}

impl From<TaskError> for HookError {
    fn from(error: TaskError) -> Self {
        HookError::Task(error)
    }
}
