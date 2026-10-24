//! The errors of an agent start: `UnknownAgentError` (`agent-hook-service.ts`) and
//! `TaskWaitingError` (`agent-runner.ts`).

use std::fmt;

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
