//! Words and small rules of the pipeline preview (`PipelinePreview.tsx`): what a node says in its
//! tooltip and to a screen reader, the runner line, the lane labels of a branch, and the text under
//! the chart. Agent labels come in as arguments.

use std::collections::HashSet;

use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::resolve::active_steps;
use natamux_proto::stages::schema::{BranchPath, Condition, Stage, StageBranch, StageGroup, Step};

use crate::stages::editor_fields::{always_waits, has_custom_prompt};
use crate::stages::editor_labels::{
    StageRunner, approval_label, branch_name, group_name, kind_label, path_name, stage_runner,
    step_name,
};

pub const NO_STAGE_ON: &str = "No stage is on. Turn on at least one.";

/// A stage waits for a person: its approval, or a click that opens the pull request. Review and
/// manual stages always do, whatever their gate says.
pub fn waits_for_person(stage: &Stage) -> bool {
    stage.gate == natamux_proto::stages::schema::Gate::Manual || always_waits(stage)
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
/// What a screen reader says for a stage, in the list and in the preview.
pub fn stage_label(stage: &Stage) -> String {
    let mut parts = vec![stage_name(stage)];
    if waits_for_person(stage) {
        parts.push("waits for approval".to_string());
    }
    if has_custom_prompt(stage) {
        parts.push("custom prompt".to_string());
    }
    parts.join(", ")
}

fn runner_name(runner: &StageRunner, agent_label: &impl Fn(&str) -> String) -> String {
    runner
        .agent
        .as_deref()
        .map_or_else(|| "task's agent".to_string(), agent_label)
}

/// The tooltip of a node.
pub fn tip_of(
    stage: &Stage,
    runner: Option<&StageRunner>,
    agent_label: impl Fn(&str) -> String,
) -> String {
    let mut parts = vec![
        kind_label(stage.kind).to_string(),
        approval_label(stage).to_string(),
    ];
    if let Some(runner) = runner {
        parts.push(format!("Agent: {}", runner_name(runner, &agent_label)));
        if let Some(model) = &runner.model {
            parts.push(format!("Model: {model}"));
        }
    }
    if let Some(output) = &stage.output {
        parts.push(format!("Output: {output}"));
    }
    if let Some(command) = &stage.command {
        parts.push(format!("Command: {command}"));
    }
    parts.join(" \u{b7} ")
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
/// The agent and model of a stage on its own agent, for a screen reader.
pub fn runner_label(
    runner: Option<&StageRunner>,
    agent_label: impl Fn(&str) -> String,
) -> Vec<String> {
    let Some(runner) = runner else {
        return Vec::new();
    };
    let agent = runner
        .agent
        .as_deref()
        .map_or_else(|| "the task's agent".to_string(), agent_label);
    let mut parts = vec![format!("runs {agent}")];
    if let Some(model) = &runner.model {
        parts.push(format!("model {model}"));
    }
    parts
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
/// The aria label of a node: the stage, then its runner.
pub fn node_label(
    stage: &Stage,
    runner: Option<&StageRunner>,
    agent_label: impl Fn(&str) -> String,
) -> String {
    let mut parts = vec![stage_label(stage)];
    parts.extend(runner_label(runner, agent_label));
    parts.join(", ")
}

/// The line under the name of a stage on its own agent: the model says more than the agent.
pub fn agent_line_text(runner: &StageRunner, agent_label: impl Fn(&str) -> String) -> String {
    runner.model.clone().unwrap_or_else(|| {
        runner
            .agent
            .as_deref()
            .map_or_else(|| "Task's agent".to_string(), agent_label)
    })
}

// The CSS chart of the web; the app draws its lines from the layout.
#[cfg_attr(not(test), allow(dead_code))]
/// The first and last nodes have no line on their outer side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    First,
    Last,
    Only,
}

// The CSS chart of the web; the app draws its lines from the layout.
#[cfg_attr(not(test), allow(dead_code))]
pub fn edge_of(index: usize, count: usize) -> Option<Edge> {
    if count == 1 {
        Some(Edge::Only)
    } else if index == 0 {
        Some(Edge::First)
    } else if index + 1 == count {
        Some(Edge::Last)
    } else {
        None
    }
}

/// The ids of the steps and stages with an error, which the preview marks.
pub fn error_ids(issues: &[PipelineIssue]) -> HashSet<String> {
    issues
        .iter()
        .filter(|issue| issue.level == IssueLevel::Error)
        .filter_map(|issue| issue.stage.clone())
        .collect()
}

// The CSS chart of the web; the app sizes each node from its own lines.
#[cfg_attr(not(test), allow(dead_code))]
/// Some stage of the steps that are on starts its own agent.
pub fn shows_agents(steps: &[Step]) -> bool {
    active_steps(steps)
        .iter()
        .flat_map(Step::stages)
        .any(|stage| stage_runner(steps, stage).is_some())
}

/// Where a task goes when no path of the branch holds.
pub fn skip_text(branch: &StageBranch, next: Option<&Step>) -> Option<String> {
    if branch.paths.iter().any(|path| path.when.is_none()) {
        return None;
    }
    Some(format!(
        "No path holds \u{2192} skip to {}",
        next.map_or_else(|| "the end".to_string(), step_name)
    ))
}

// The web's lane label; the app shows `when_tip` in the chip.
#[cfg_attr(not(test), allow(dead_code))]
/// The label of a lane: its name and condition in words.
pub fn when_label(path: &BranchPath) -> String {
    let name = path_name(path);
    match &path.when {
        None => format!("{name} Otherwise"),
        Some(when) => {
            let not = if path.not == Some(true) { "NOT " } else { "" };
            match when {
                Condition::Command { run } => format!("{name} if {not}{run} succeeds"),
                Condition::Output {
                    file,
                    section,
                    contains,
                } => {
                    let target = section.as_ref().map_or_else(
                        || file.clone(),
                        |section| format!("{file} \u{203a} {section}"),
                    );
                    format!("{name} if {not}{target} has \"{contains}\"")
                }
            }
        }
    }
}

/// The tooltip of a lane label.
pub fn when_tip(path: &BranchPath) -> String {
    describe_condition(path)
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
pub fn branch_button_label(branch: &StageBranch) -> String {
    format!(
        "Branch {}, one path of {}",
        branch_name(branch),
        branch.paths.len()
    )
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
pub fn branch_sub_text(branch: &StageBranch) -> String {
    format!("One path of {}, checked in order", branch.paths.len())
}

pub fn fork_label(branch: &StageBranch) -> String {
    format!(
        "{}, {} paths, checked in order",
        branch_name(branch),
        branch.paths.len()
    )
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
pub fn path_stages_label(path: &BranchPath) -> String {
    format!("{}, stages", path_name(path))
}

pub fn group_list_label(group: &StageGroup) -> String {
    format!("{}, side by side", group_name(group))
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
pub fn paths_list_label(branch: &StageBranch) -> String {
    format!("Paths of {}", branch_name(branch))
}

/// The line over the paths in the words under the chart.
pub fn branch_note_head(branch: &StageBranch) -> String {
    format!(
        "{} \u{b7} one path of {}, checked in order:",
        branch_name(branch),
        branch.paths.len()
    )
}

/// One path in the words under the chart: "Small change · plan.md "Size" contains "small" →
/// Implement, Automated tests".
pub fn path_note(path: &BranchPath) -> String {
    let stages: Vec<String> = path.stages.iter().map(stage_name).collect();
    format!(
        "{} \u{b7} {} \u{2192} {}",
        path_name(path),
        describe_condition(path),
        stages.join(", ")
    )
}

#[cfg(test)]
#[path = "preview_rules_tests.rs"]
mod tests;
