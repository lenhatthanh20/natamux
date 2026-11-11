//! The folding groups of the stage sheet's right column (T-0313, mockup B): which values each
//! holds, how many of them differ from the default, and whether one of them has a problem.

use natamux_proto::stages::catalog::catalog_stage;
use natamux_proto::stages::context_rules::AFTER_CONTEXT;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{Gate, Stage, StageKind};

/// A folding group of the right column. Done check starts open, the others folded.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SheetGroup {
    /// Check command, Finished when.
    DoneCheck,
    /// Arguments, Command (set on a stage that does not run it), Output file, Required sections.
    /// A testing stage runs its command: it sits over the prompt, so Files never holds it.
    Files,
    /// Workflow, CI, a terminal of its own.
    Options,
}

impl SheetGroup {
    pub const ALL: [SheetGroup; 3] = [
        SheetGroup::DoneCheck,
        SheetGroup::Files,
        SheetGroup::Options,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SheetGroup::DoneCheck => "Done check",
            SheetGroup::Files => "Files and arguments",
            SheetGroup::Options => "Options",
        }
    }

    pub fn open_at_first(self) -> bool {
        self == SheetGroup::DoneCheck
    }

    pub fn index(self) -> usize {
        match self {
            SheetGroup::DoneCheck => 0,
            SheetGroup::Files => 1,
            SheetGroup::Options => 2,
        }
    }
}

/// Files holds the Command only when the stage does not run it. A testing stage runs it, and
/// the sheet draws it at the left (`sheet_rules::command_at_left`).
fn command_in_files(stage: &Stage) -> bool {
    stage.kind != StageKind::Testing
}

fn filled(text: &Option<String>) -> bool {
    text.as_deref().is_some_and(|text| !text.is_empty())
}

/// How many values of `group` differ from the default: a text field that is not empty, or a
/// flag that differs from the catalog's value (false for a stage of your own).
pub fn changed_in(stage: &Stage, group: SheetGroup) -> usize {
    let base =
        catalog_stage(&stage.id).unwrap_or_else(|| Stage::new(&stage.id, stage.kind, Gate::Manual));
    let texts: Vec<(&Option<String>, &Option<String>)> = match group {
        SheetGroup::DoneCheck => vec![
            (&stage.check, &base.check),
            (&stage.done_when, &base.done_when),
        ],
        SheetGroup::Files => {
            let mut texts = vec![(&stage.args, &base.args), (&stage.output, &base.output)];
            if command_in_files(stage) {
                texts.push((&stage.command, &base.command));
            }
            texts
        }
        SheetGroup::Options => Vec::new(),
    };
    let texts = texts
        .iter()
        .filter(|(now, default)| now != default && (filled(now) || filled(default)))
        .count();
    let listed = |list: &Option<Vec<String>>| list.as_ref().is_some_and(|list| !list.is_empty());
    let sections = group == SheetGroup::Files
        && stage.sections != base.sections
        && (listed(&stage.sections) || listed(&base.sections));
    let flags = match group {
        SheetGroup::Options => vec![
            (stage.workflow, base.workflow),
            (stage.ci, base.ci),
            (stage.own_shell, base.own_shell),
        ],
        _ => Vec::new(),
    };
    let flags = flags
        .iter()
        .filter(|(now, default)| now.unwrap_or(false) != default.unwrap_or(false))
        .count();
    texts + usize::from(sections) + flags
}

/// The words after a folded group's title: "· 2 changed" or "· all default".
pub fn group_summary(changed: usize) -> String {
    if changed == 0 {
        "\u{b7} all default".to_owned()
    } else {
        format!("\u{b7} {changed} changed")
    }
}

/// The issue messages name the field as a prefix ("Command: ...") or by the rule's own words,
/// since `PipelineIssue` has no field. Name, prompt, agent and end-terminal problems are in the
/// Stage card, which never folds.
fn group_words(group: SheetGroup) -> &'static [&'static str] {
    match group {
        SheetGroup::DoneCheck => &["Check command:", "Finished when:"],
        SheetGroup::Files => &["Arguments:", "Output file:", "Required sections:"],
        SheetGroup::Options => &["in its own terminal", "checked on CI", "with a workflow"],
    }
}

/// The words of a Command problem: in Files only when `command_in_files`.
const COMMAND_WORDS: [&str; 2] = ["Command:", "Give the command to run"];

const OWN_AGENT_OR_ARGS: &str = "its own agent or arguments";

/// The stage after the context names its agent, arguments and terminal in one message: open
/// the group of each one it has set (T-0333).
fn after_context_field(stage: &Stage, issue: &PipelineIssue, group: SheetGroup) -> bool {
    let set = match group {
        SheetGroup::Options => stage.own_shell == Some(true),
        SheetGroup::Files => filled(&stage.args),
        SheetGroup::DoneCheck => false,
    };
    set && issue.message.contains(AFTER_CONTEXT)
}

/// True when an issue of `stage` is about a field of `group`, so the group opens on its own.
pub fn group_has_issue(stage: &Stage, issues: &[PipelineIssue], group: SheetGroup) -> bool {
    issues.iter().any(|issue| {
        issue.stage.as_deref().is_none_or(|id| id == stage.id)
            && (group_words(group)
                .iter()
                .any(|word| issue.message.contains(word))
                || (group == SheetGroup::Files
                    && command_in_files(stage)
                    && COMMAND_WORDS.iter().any(|word| issue.message.contains(word)))
                // It names agent and arguments; Arguments is in Files, so open when it is set.
                || (group == SheetGroup::Files
                    && filled(&stage.args)
                    && issue.message.contains(OWN_AGENT_OR_ARGS))
                || after_context_field(stage, issue, group))
    })
}

#[cfg(test)]
#[path = "advanced_rules_tests.rs"]
mod tests;
