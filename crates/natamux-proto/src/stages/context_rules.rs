//! The context stage opens every pipeline, and the stage after it goes on with the task's agent
//! in Terminal 1, so it keeps the session that the context started (T-0333).

use crate::stages::position::code_stage;
use crate::stages::resolve::active_steps;
use crate::stages::schema::{Stage, StageKind, Step};

pub const NO_CONTEXT: &str = "A pipeline starts with the context stage";
pub const CONTEXT_OFF: &str = "The context stage cannot be turned off";
/// Words of the message about the stage after the context, so the sheet can find it.
pub const AFTER_CONTEXT: &str = "comes right after the context stage";

/// The context rules, as (stage, message). They run after the other rules pass, so a context in
/// the wrong place gives one message.
pub(crate) fn context_problems(steps: &[Step]) -> Vec<(Option<String>, String)> {
    let contexts: Vec<&Stage> = steps
        .iter()
        .flat_map(Step::stages)
        .filter(|stage| stage.kind == StageKind::Context)
        .collect();
    let Some(context) = contexts.first() else {
        return vec![(None, NO_CONTEXT.into())];
    };
    if !context.is_on() {
        return vec![(Some(context.id.clone()), CONTEXT_OFF.into())];
    }
    let active = active_steps(steps);
    after_context(&active)
        .into_iter()
        .filter_map(|stage| {
            task_agent_problem(stage).map(|message| (Some(stage.id.clone()), message))
        })
        .collect()
}

/// The ids of the stages that run right after the context: the stage, the stage of a group that
/// keeps Terminal 1, or the first of each path of a branch. Only stages that are on count.
pub fn after_context_stages(steps: &[Step]) -> Vec<String> {
    let active = active_steps(steps);
    after_context(&active)
        .into_iter()
        .map(|stage| stage.id.clone())
        .collect()
}

fn after_context(active: &[Step]) -> Vec<&Stage> {
    let first_is_context =
        matches!(active.first(), Some(Step::Stage(stage)) if stage.kind == StageKind::Context);
    let Some(next) = active.get(1).filter(|_| first_is_context) else {
        return Vec::new();
    };
    let firsts: Vec<&Step> = match next {
        Step::Branch(branch) => branch
            .paths
            .iter()
            .filter_map(|path| path.stages.first())
            .collect(),
        other => vec![other],
    };
    firsts.into_iter().filter_map(code_stage).collect()
}

/// Why `stage` cannot run with the task's agent in Terminal 1, if it cannot. The runner starts
/// the task's agent only for an agent stage and a testing stage without a command.
fn task_agent_problem(stage: &Stage) -> Option<String> {
    let id = &stage.id;
    let starts_agent = match stage.kind {
        StageKind::Agent => true,
        StageKind::Testing => stage.command.is_none(),
        _ => false,
    };
    if !starts_agent {
        let what = if stage.kind == StageKind::Testing {
            "a testing stage with a command".to_owned()
        } else {
            format!("a {} stage", stage.kind.as_str())
        };
        return Some(format!(
            "The stage right after the context stage must be an agent stage, and \"{id}\" is {what}"
        ));
    }
    let own = stage.agent.is_some() || stage.args.is_some() || stage.own_shell == Some(true);
    own.then(|| {
        format!(
            "\"{id}\" {AFTER_CONTEXT}, so it runs with the task's agent in Terminal 1. Clear its own agent, arguments and terminal"
        )
    })
}

#[cfg(test)]
#[path = "context_rules_tests.rs"]
mod tests;
