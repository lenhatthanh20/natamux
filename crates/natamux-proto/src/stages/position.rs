//! Where a stage sits in a pipeline (`packages/core/src/stages/position.ts`): the step of a stage,
//! the path of a stage, and the words for a path's condition.

use serde_json::{Map, Value};

use crate::stages::schema::{BranchPath, Condition, Stage, StageBranch, StageKind, Step};

/// The index of the step that holds stage or step `id` (`stepIndex`, which gives -1 for none).
pub fn step_index(steps: &[Step], id: &str) -> Option<usize> {
    steps
        .iter()
        .position(|step| step.id() == id || step.stages().iter().any(|stage| stage.id == id))
}

/// `findStep`: the step that holds stage or step `id`.
pub fn find_step<'a>(steps: &'a [Step], id: &str) -> Option<&'a Step> {
    step_index(steps, id).map(|index| &steps[index])
}

/// `stepToRun`: the step that runs for stage or step `id`: its group, or the stage itself, also
/// inside a path.
pub fn step_to_run(steps: &[Step], id: &str) -> Option<Step> {
    let step = find_step(steps, id)?;
    if step.is_branch() {
        let found = step.stages().into_iter().find(|stage| stage.id == id);
        return Some(found.map_or_else(|| step.clone(), |stage| Step::Stage(stage.clone())));
    }
    Some(step.clone())
}

/// `codeStage`: the stage of a group that changes code keeps the first terminal; each other stage
/// gets the next one.
pub fn code_stage(step: &Step) -> Option<&Stage> {
    let stages = step.stages();
    let agents: Vec<&Stage> = stages
        .iter()
        .copied()
        .filter(|stage| stage.kind == StageKind::Agent)
        .collect();
    agents
        .iter()
        .copied()
        .find(|stage| stage.output.is_none())
        .or_else(|| agents.first().copied())
        .or_else(|| stages.first().copied())
}

/// The branch and path of a stage in a path, and where the stage is in the path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathPlace<'a> {
    pub branch: &'a StageBranch,
    pub path: &'a BranchPath,
    pub index: usize,
}

pub fn path_of<'a>(steps: &'a [Step], stage_id: &str) -> Option<PathPlace<'a>> {
    steps.iter().find_map(|step| {
        let Step::Branch(branch) = step else {
            return None;
        };
        branch.paths.iter().find_map(|path| {
            let index = path.stages.iter().position(|stage| stage.id == stage_id)?;
            Some(PathPlace {
                branch,
                path,
                index,
            })
        })
    })
}

/// A stage of a branch where the task took another path, or none. `paths` is `Task.paths`:
/// branch id to path id, "" for a branch the task skipped.
pub fn is_not_taken(steps: &[Step], paths: &Map<String, Value>, stage_id: &str) -> bool {
    let Some(place) = path_of(steps, stage_id) else {
        return false;
    };
    match paths.get(&place.branch.id).and_then(Value::as_str) {
        Some(taken) => taken != place.path.id,
        None => false,
    }
}

/// The condition of a path in words, for the settings chart and the reason a task took it.
pub fn describe_condition(path: &BranchPath) -> String {
    let not = path.not == Some(true);
    match &path.when {
        None => "Otherwise".into(),
        Some(Condition::Command { run }) => {
            format!(
                "{run} {}",
                if not { "does not succeed" } else { "succeeds" }
            )
        }
        Some(Condition::Output {
            file,
            section,
            contains,
        }) => {
            let place = section
                .as_ref()
                .map_or_else(|| file.clone(), |section| format!("{file} \"{section}\""));
            let verb = if not { "does not contain" } else { "contains" };
            format!("{place} {verb} \"{contains}\"")
        }
    }
}

#[cfg(test)]
#[path = "position_tests.rs"]
mod tests;
