//! Pure rules of how the Start a task dialog reads (T-0292, option B): the footer note, the help
//! lines, the footnotes under the cards, and which blocked reason shows as a red line.

use natamux_proto::projects::repo::BranchList;
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::resolve::active_steps;
use natamux_proto::stages::schema::Step;

use super::new_workspace_rules::{
    SourceChoice, StartForm, blocked_reason, start_branch, start_hint,
};
use crate::agents::agent_choices::{DEFAULT_AGENT, agent_label};

/// Blocked reasons that belong to a row, so they also show as a red line under it.
pub const CHOOSE_AGENT: &str = "Choose an agent for this task flow";
pub const PICK_AGENT: &str = "Pick an agent to run the skill";
/// The Branch from text while the branches load.
const DEFAULT_BASE: &str = "The default branch";

/// The stage names a new task gets, in order: groups and branch paths flattened, stages that
/// are off left out.
pub fn stage_names(steps: &[Step]) -> Vec<String> {
    let active = active_steps(steps);
    active
        .iter()
        .flat_map(Step::stages)
        .map(stage_name)
        .collect()
}

fn stage_count(stages: usize) -> String {
    match stages {
        // The settings are not read yet.
        0 => "full flow".into(),
        1 => "1 stage".into(),
        n => format!("{n} stages"),
    }
}

/// The line left of the footer buttons: why Start task is off, else what it will make.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FooterNote {
    Blocked(&'static str),
    Plan(String),
}

pub fn footer_note(
    form: &StartForm,
    choice: &SourceChoice,
    base: &str,
    stages: usize,
) -> FooterNote {
    if let Some(reason) = blocked_reason(form, choice) {
        return FooterNote::Blocked(reason);
    }
    let place = if form.no_worktree {
        "Task only, no workspace yet".to_string()
    } else {
        format!("New worktree from {base}")
    };
    let agent = if form.agent.is_empty() {
        "plain terminal".to_string()
    } else {
        agent_label(Some(&form.agent))
    };
    let flow = if form.single_agent {
        "one agent".to_string()
    } else {
        stage_count(stages)
    };
    FooterNote::Plan(format!("{place} · {agent} · {flow}"))
}

/// The help under Branch. `follows`: the title gives a name.
pub fn branch_help(edited: bool, follows: bool) -> &'static str {
    if edited {
        "Your own name"
    } else if follows {
        "Follows the title"
    } else {
        "Natamux names it"
    }
}

pub fn worktree_help(on: bool) -> &'static str {
    if on {
        "A git worktree on a new branch, apart from your checkout."
    } else {
        "Task only. Make the workspace later from the task."
    }
}

pub fn worktree_tip(on: bool) -> &'static str {
    if on {
        "On: a git worktree on a new branch"
    } else {
        "Off: the task only, no workspace yet"
    }
}

/// A piece of a footnote line: plain text, or a name in the mono font.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FootPart {
    Text(String),
    Mono(String),
}

/// The footnote under the Workspace card.
pub fn workspace_foot(on: bool, branch: &str, base: &str) -> Vec<FootPart> {
    if !on {
        return vec![FootPart::Text(
            "No workspace yet: the task gets an id and waits. Make the workspace later from the \
             task."
                .into(),
        )];
    }
    let branch = branch.trim();
    let named = if branch.is_empty() {
        FootPart::Text("a new branch".into())
    } else {
        FootPart::Mono(branch.into())
    };
    vec![
        FootPart::Text("A new worktree on ".into()),
        named,
        FootPart::Text(", made from ".into()),
        FootPart::Mono(base.into()),
        FootPart::Text(".".into()),
    ]
}

/// The footnote under the Agent card; a full flow has none, the footer note counts its stages.
pub fn agent_foot(single: bool) -> Option<&'static str> {
    single.then_some(
        "One agent does the whole task in Terminal 1. The pipeline stages do not run.",
    )
}

/// The row whose red line says the blocked reason.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ErrorRow {
    Skill,
    Flow,
}

/// Only the two agent reasons belong to a row; the others show in the footer note only.
pub fn inline_error(reason: Option<&str>) -> Option<ErrorRow> {
    match reason? {
        CHOOSE_AGENT => Some(ErrorRow::Flow),
        PICK_AGENT => Some(ErrorRow::Skill),
        _ => None,
    }
}

/// The branch the workspace starts from: the picked one, else the start one.
pub fn base_name(list: Option<&BranchList>, base: &str) -> String {
    if !base.is_empty() {
        return base.into();
    }
    start_branch(list).unwrap_or_else(|| DEFAULT_BASE.into())
}

/// "· default" after the Branch from text: nothing is picked and the start branch is the
/// project's default one.
pub fn shows_default(list: Option<&BranchList>, base: &str) -> bool {
    base.is_empty() && list.is_some_and(|list| list.default_branch.is_some())
}

/// The tooltip of Start task.
pub fn start_tip(reason: Option<&'static str>, busy: bool, no_worktree: bool) -> &'static str {
    if busy {
        return "Starting the task";
    }
    reason.unwrap_or_else(|| start_hint(no_worktree))
}

/// The header of the Skill menu: the skills are those of the agent that runs them.
pub fn skill_menu_header(agent: &str) -> String {
    let agent = if agent.is_empty() {
        DEFAULT_AGENT
    } else {
        agent
    };
    format!("{} · this project", agent_label(Some(agent)))
}

/// The Lucide icon of a context source.
pub fn source_icon(choice: &SourceChoice) -> &'static str {
    match choice.kind() {
        "manual" => "pencil",
        "github-issue" => "circle-dot",
        "mcp" => "plug",
        _ => "sparkles",
    }
}

#[cfg(test)]
#[path = "new_workspace_view_rules_tests.rs"]
mod tests;
