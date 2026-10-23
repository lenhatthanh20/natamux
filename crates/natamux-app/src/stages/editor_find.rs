//! Finding things in the steps of the pipeline editor (`pipeline-editor/find.ts`): ids, places
//! and the small list edits the other editor rules share.

use std::collections::HashSet;

use natamux_proto::stages::schema::{GROUP_KINDS, Stage, StageBranch, StageGroup, StageKind, Step};

use crate::stages::editor_draft::MAX_ID;

/// Every id in the pipeline: stages, groups, branches and paths.
pub fn used_ids(steps: &[Step]) -> HashSet<String> {
    let mut used = HashSet::new();
    for step in steps {
        used.insert(step.id().to_string());
        match step {
            Step::Group(group) => used.extend(group.parallel.iter().map(|stage| stage.id.clone())),
            Step::Branch(branch) => {
                for path in &branch.paths {
                    used.insert(path.id.clone());
                    used.extend(path.stages.iter().map(|stage| stage.id.clone()));
                }
            }
            Step::Stage(_) => {}
        }
    }
    used
}

/// Where a stage is: its step; its lane when it runs in a group; its path and its place in the
/// path when it is in a branch. A group's or a branch's own id has none of these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Place {
    pub index: usize,
    pub lane: Option<usize>,
    pub path: Option<usize>,
    pub slot: Option<usize>,
}

pub fn find_place(steps: &[Step], id: &str) -> Option<Place> {
    for (index, step) in steps.iter().enumerate() {
        let place = |lane, path, slot| Place {
            index,
            lane,
            path,
            slot,
        };
        if step.id() == id {
            return Some(place(None, None, None));
        }
        match step {
            Step::Group(group) => {
                if let Some(lane) = group.parallel.iter().position(|stage| stage.id == id) {
                    return Some(place(Some(lane), None, None));
                }
            }
            Step::Branch(branch) => {
                for (path, item) in branch.paths.iter().enumerate() {
                    if let Some(slot) = item.stages.iter().position(|stage| stage.id == id) {
                        return Some(place(None, Some(path), Some(slot)));
                    }
                }
            }
            Step::Stage(_) => {}
        }
    }
    None
}

pub fn find_branch<'a>(steps: &'a [Step], id: &str) -> Option<&'a StageBranch> {
    match steps.iter().find(|step| step.id() == id) {
        Some(Step::Branch(branch)) => Some(branch),
        _ => None,
    }
}

/// The step that holds a path, and the place of the path in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathAt {
    pub index: usize,
    pub path: usize,
}

pub fn find_path(steps: &[Step], path_id: &str) -> Option<PathAt> {
    steps
        .iter()
        .enumerate()
        .find_map(|(index, step)| match step {
            Step::Branch(branch) => branch
                .paths
                .iter()
                .position(|item| item.id == path_id)
                .map(|path| PathAt { index, path }),
            _ => None,
        })
}

/// `GROUP_KINDS.includes(kind)`.
pub fn runs_in_group(kind: StageKind) -> bool {
    GROUP_KINDS.contains(&kind)
}

/// A path holds single stages; a context or release stage has its fixed place and cannot be in one.
pub fn stage_fits_in_path(stage: &Stage) -> bool {
    stage.kind != StageKind::Context && stage.kind != StageKind::Release
}

pub fn fits_in_path(step: &Step) -> bool {
    matches!(step, Step::Stage(stage) if stage_fits_in_path(stage))
}

pub fn find_stage<'a>(steps: &'a [Step], id: &str) -> Option<&'a Stage> {
    steps
        .iter()
        .flat_map(Step::stages)
        .find(|stage| stage.id == id)
}

/// The group a stage runs in, if any.
pub fn group_of<'a>(steps: &'a [Step], id: &str) -> Option<&'a StageGroup> {
    let place = find_place(steps, id)?;
    place.lane?;
    match steps.get(place.index) {
        Some(Step::Group(group)) => Some(group),
        _ => None,
    }
}

/// The steps with the one at `index` swapped for `replacement`, or taken out when it is `None`.
pub fn replace_at(steps: &[Step], index: usize, replacement: Option<Step>) -> Vec<Step> {
    let mut next = steps[..index.min(steps.len())].to_vec();
    next.extend(replacement);
    next.extend(steps.iter().skip(index + 1).cloned());
    next
}

pub fn insert_at(steps: &[Step], index: usize, step: Step) -> Vec<Step> {
    let at = index.min(steps.len());
    let mut next = steps[..at].to_vec();
    next.push(step);
    next.extend(steps[at..].iter().cloned());
    next
}

/// The base with a number after it, when the base is used already.
pub fn free_id(used: &HashSet<String>, base: &str) -> String {
    if !used.contains(base) {
        return base.to_string();
    }
    let mut number = 2;
    while used.contains(&format!("{base}-{number}")) {
        number += 1;
    }
    format!("{base}-{number}")
}

/// "path-3", counting up from `from` until no id of the pipeline is the same.
pub fn free_path_id(used: &HashSet<String>, from: usize) -> String {
    let mut number = from;
    while used.contains(&format!("path-{number}")) {
        number += 1;
    }
    format!("path-{number}")
}

/// A group id that no stage or group uses yet, from the id of its first stage.
pub fn new_group_id(steps: &[Step], first: &str) -> String {
    let base = if first.len() <= MAX_ID - 10 {
        format!("{first}-group")
    } else {
        "group".to_string()
    };
    free_id(&used_ids(steps), &base)
}

/// The place of the release stage, or the end when there is none.
pub fn release_index(steps: &[Step]) -> usize {
    steps
        .iter()
        .position(|step| matches!(step, Step::Stage(stage) if stage.kind == StageKind::Release))
        .unwrap_or(steps.len())
}

pub fn step_of_stage<'a>(steps: &'a [Step], id: &str) -> Option<&'a Step> {
    steps
        .iter()
        .find(|step| step.stages().iter().any(|stage| stage.id == id))
}

#[cfg(test)]
#[path = "editor_find_tests.rs"]
mod tests;
