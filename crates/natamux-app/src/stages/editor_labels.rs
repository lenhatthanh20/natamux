//! Words of the pipeline editor (`pipeline-editor/labels.ts`): kind and approval labels, names of
//! steps, and what a screen reader says for the pipeline.

use natamux_proto::stages::capabilities::{stage_name, stage_name_of_id};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::resolve::active_steps;
use natamux_proto::stages::schema::{
    BranchPath, Gate, Stage, StageBranch, StageGroup, StageKind, Step,
};

use crate::stages::editor_fields::{always_waits, takes_own_agent};

pub fn kind_label(kind: StageKind) -> &'static str {
    match kind {
        StageKind::Context => "Context",
        StageKind::Agent => "Agent",
        StageKind::Review => "Code review",
        StageKind::Testing => "Testing",
        StageKind::Release => "Release",
        StageKind::Manual => "Check by a person",
    }
}

pub fn approval_label(stage: &Stage) -> &'static str {
    if always_waits(stage) {
        return "Always waits for you";
    }
    match (stage.kind, stage.gate) {
        (StageKind::Release, Gate::Manual) => "Opens the pull request when you click",
        (StageKind::Release, Gate::Auto) => "Opens the pull request by itself",
        (_, Gate::Manual) => "Waits for your approval",
        (_, Gate::Auto) => "Moves on by itself",
    }
}

/// The model in the words of an agent's command line: `--model x`, `--model=x` or `-m x`.
pub fn model_of(args: Option<&str>) -> Option<String> {
    let words: Vec<&str> = args?.split_whitespace().collect();
    for (index, word) in words.iter().enumerate() {
        if let Some(model) = word.strip_prefix("--model=") {
            return (!model.is_empty()).then(|| model.to_string());
        }
        if *word == "--model" || *word == "-m" {
            return words
                .get(index + 1)
                .filter(|next| !next.starts_with('-'))
                .map(|next| next.to_string());
        }
    }
    None
}

/// The agent and model of a stage that starts its own agent. No agent is the task's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StageRunner {
    pub agent: Option<String>,
    pub model: Option<String>,
}

/// Nothing for a stage that uses the task's agent, or whose agent and args are not used.
pub fn stage_runner(steps: &[Step], stage: &Stage) -> Option<StageRunner> {
    if (stage.agent.is_none() && stage.args.is_none()) || !takes_own_agent(steps, &stage.id) {
        return None;
    }
    Some(StageRunner {
        agent: stage.agent.clone(),
        model: model_of(stage.args.as_deref()),
    })
}

/// "A, B and C"
pub fn join_names(names: &[String]) -> String {
    match names {
        [] => String::new(),
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
    }
}

fn name_of(name: Option<&str>, id: &str) -> String {
    name.map_or_else(|| stage_name_of_id(id), str::to_string)
}

pub fn group_name(group: &StageGroup) -> String {
    group
        .name
        .clone()
        .unwrap_or_else(|| join_names(&group.parallel.iter().map(stage_name).collect::<Vec<_>>()))
}

pub fn branch_name(branch: &StageBranch) -> String {
    name_of(branch.name.as_deref(), &branch.id)
}

pub fn path_name(path: &BranchPath) -> String {
    name_of(path.name.as_deref(), &path.id)
}

/// The name of a step in a sentence: a stage, a group or a branch.
pub fn step_name(step: &Step) -> String {
    match step {
        Step::Group(group) => group_name(group),
        Step::Branch(branch) => branch_name(branch),
        Step::Stage(stage) => stage_name(stage),
    }
}

/// "Small change if plan.md contains "small"", or "Review PR, otherwise".
pub fn path_label(path: &BranchPath) -> String {
    if path.when.is_none() {
        format!("{}, otherwise", path_name(path))
    } else {
        format!("{} if {}", path_name(path), describe_condition(path))
    }
}

/// "After plan, one path of 2: Small change if …; Review PR, otherwise"
pub fn branch_label(branch: &StageBranch) -> String {
    let paths: Vec<String> = branch.paths.iter().map(path_label).collect();
    format!(
        "{}, one path of {}: {}",
        branch_name(branch),
        branch.paths.len(),
        paths.join("; ")
    )
}

// A screen-reader label of the web: GPUI has no accessibility API yet.
#[cfg_attr(not(test), allow(dead_code))]
/// What the preview says to a screen reader: the stages that are on, in order.
pub fn pipeline_label(steps: &[Step]) -> String {
    let parts: Vec<String> = active_steps(steps)
        .iter()
        .map(|step| match step {
            Step::Branch(branch) => branch_label(branch),
            Step::Group(group) => format!(
                "{} side by side",
                join_names(&group.parallel.iter().map(stage_name).collect::<Vec<_>>())
            ),
            Step::Stage(stage) => stage_name(stage),
        })
        .collect();
    if parts.is_empty() {
        "Pipeline: no stage is on".to_string()
    } else {
        format!("Pipeline: {}", parts.join(", then "))
    }
}

#[cfg(test)]
#[path = "editor_labels_tests.rs"]
mod tests;
