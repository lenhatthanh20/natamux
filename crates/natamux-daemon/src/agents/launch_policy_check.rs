//! The word check of a start. In the `ask` permission mode, and always in Strict mode, a word
//! that turns off the agent's approvals fails the start with `agent.policy-blocked`, before any
//! file is written or anything is typed. Natamux never drops the word and starts anyway.

use natamux_agents::launch_policy::forbidden_word;
use natamux_proto::agents::start::LAUNCHABLE_AGENTS;
use serde_json::json;

use crate::agents::errors::AgentError;
use crate::tasks::service::TaskService;
use crate::workspaces::setup_script::WORKSPACE_WARNING_EVENT;

pub const POLICY_BLOCKED_EVENT: &str = "agent.policy-blocked";

/// Logged when an agent on base "other" starts in the `ask` permission mode.
pub const OTHER_NOTICE: &str =
    "Natamux cannot check this agent's flags, so it cannot hold it to the ask permission mode.";

const OTHER_STRICT_REASON: &str =
    "Strict mode does not start an agent on base \"other\": Natamux cannot check its flags.";

/// The words of a start and where each came from.
pub struct StartWords<'a> {
    pub kind: &'a str,
    /// `ask` permission mode, or Strict mode.
    pub ask: bool,
    pub strict: bool,
    pub sources: [(&'a str, Option<&'a str>); 3],
}

/// `NATAMUX_<AGENT>_ARGS` of a built-in kind, as `config.rs` reads it.
pub fn args_variable(kind: &str) -> String {
    let name = if kind == "claude-code" {
        "CLAUDE".to_string()
    } else {
        kind.to_uppercase()
    };
    format!("NATAMUX_{name}_ARGS")
}

fn block(tasks: &TaskService, task: &str, word: &str, source: &str, reason: &str) -> AgentError {
    let payload = json!({ "word": word, "source": source, "reason": reason });
    let logged = tasks.record_event(task, POLICY_BLOCKED_EVENT, payload, "system");
    if let Err(error) = logged {
        eprintln!("The blocked start of {task} is not logged: {error}");
    }
    natamux_core::privacy::audit::record("agent.blocked", task, "policy");
    AgentError::Untrusted(reason.to_string())
}

/// Fails the start on the first forbidden word. An agent on base "other" runs in the `ask`
/// mode with a notice (the user picked the program), and is blocked in Strict mode.
pub fn check_words(
    tasks: &TaskService,
    task_id: &str,
    words: &StartWords<'_>,
) -> Result<(), AgentError> {
    if !words.ask && !words.strict {
        return Ok(());
    }
    if !LAUNCHABLE_AGENTS.contains(&words.kind) {
        if words.strict {
            return Err(block(tasks, task_id, "", "agent", OTHER_STRICT_REASON));
        }
        let payload = json!({ "message": OTHER_NOTICE });
        let logged = tasks.record_event(task_id, WORKSPACE_WARNING_EVENT, payload, "system");
        if let Err(error) = logged {
            eprintln!("The notice of {task_id} is not logged: {error}");
        }
        return Ok(());
    }
    for (source, text) in words.sources {
        if let Some(hit) = text.and_then(|text| forbidden_word(words.kind, text)) {
            return Err(block(tasks, task_id, &hit.word, source, &hit.reason));
        }
    }
    Ok(())
}
