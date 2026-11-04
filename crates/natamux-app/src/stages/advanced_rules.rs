//! What the Advanced block of the stage details needs to know: how many of its values differ from
//! the default, and whether one of them has a problem.

use natamux_proto::stages::catalog::catalog_stage;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{Gate, Stage};

fn filled(text: &Option<String>) -> bool {
    text.as_deref().is_some_and(|text| !text.is_empty())
}

/// How many Advanced values differ from the default: a text field that is not empty, or a flag
/// that differs from the catalog's value (false for a stage of your own).
pub fn changed_count(stage: &Stage) -> usize {
    let base =
        catalog_stage(&stage.id).unwrap_or_else(|| Stage::new(&stage.id, stage.kind, Gate::Manual));
    let texts = [
        (&stage.args, &base.args),
        (&stage.command, &base.command),
        (&stage.check, &base.check),
        (&stage.output, &base.output),
        (&stage.done_when, &base.done_when),
    ]
    .iter()
    .filter(|(now, default)| now != default && (filled(now) || filled(default)))
    .count();
    let listed = |list: &Option<Vec<String>>| list.as_ref().is_some_and(|list| !list.is_empty());
    let sections =
        stage.sections != base.sections && (listed(&stage.sections) || listed(&base.sections));
    let flags = [
        (stage.workflow, base.workflow),
        (stage.ci, base.ci),
        (stage.own_shell, base.own_shell),
    ]
    .iter()
    .filter(|(now, default)| now.unwrap_or(false) != default.unwrap_or(false))
    .count();
    texts + usize::from(sections) + flags
}

/// The issue messages name the field as a prefix ("Command: ...") or by the rule's own words,
/// since `PipelineIssue` has no field. Name, prompt, agent and end-terminal problems stay Basic.
const ADVANCED_WORDS: [&str; 10] = [
    "Arguments:",
    "Command:",
    "Check command:",
    "Output file:",
    "Required sections:",
    "Finished when:",
    "Give the command to run",
    "in its own terminal",
    "checked on CI",
    "with a workflow",
];

const OWN_AGENT_OR_ARGS: &str = "its own agent or arguments";

/// True when an issue of `stage` is about an Advanced field.
pub fn advanced_has_issue(stage: &Stage, issues: &[PipelineIssue]) -> bool {
    issues.iter().any(|issue| {
        issue.stage.as_deref().is_none_or(|id| id == stage.id)
            && (ADVANCED_WORDS
                .iter()
                .any(|word| issue.message.contains(word))
                // It names agent and arguments; Arguments is Advanced, so open when it is set.
                || (filled(&stage.args) && issue.message.contains(OWN_AGENT_OR_ARGS)))
    })
}

#[cfg(test)]
#[path = "advanced_rules_tests.rs"]
mod tests;
