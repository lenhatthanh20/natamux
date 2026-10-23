//! Moving stages in the pipeline editor (`pipeline-editor/move.ts`): up and down, drag and drop,
//! running side by side, and removing. Each edit takes the steps and gives new ones; when nothing
//! can change, the steps come back equal.

use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::schema::{Stage, StageBranch, StageGroup, Step};

use crate::stages::editor_draft::MAX_GROUP;
use crate::stages::editor_find::{
    PathAt, Place, find_path, find_place, fits_in_path, insert_at, new_group_id, replace_at,
    runs_in_group, stage_fits_in_path,
};
use crate::stages::editor_labels::branch_name;

fn with_path_stages(branch: &StageBranch, path: usize, stages: Vec<Stage>) -> Step {
    let mut next = branch.clone();
    if let Some(item) = next.paths.get_mut(path) {
        item.stages = stages;
    }
    Step::Branch(next)
}

/// A group of one stage is no group: the stage runs on its own.
fn regroup(group: &StageGroup, parallel: Vec<Stage>) -> Step {
    match <[Stage; 1]>::try_from(parallel) {
        Ok([only]) => Step::Stage(only),
        Err(parallel) => Step::Group(StageGroup {
            parallel,
            ..group.clone()
        }),
    }
}

/// A step or a stage taken out. `at` is where the rest has the step it came from.
struct TakenOut {
    rest: Vec<Step>,
    taken: Step,
    at: usize,
}

fn take_out(steps: &[Step], id: &str) -> Option<TakenOut> {
    let place = find_place(steps, id)?;
    let step = steps.get(place.index)?;
    let at = place.index;
    if let (Some(path), Some(slot), Step::Branch(branch)) = (place.path, place.slot, step) {
        let stages = &branch.paths.get(path)?.stages;
        let taken = Step::Stage(stages.get(slot)?.clone());
        let left = stages
            .iter()
            .enumerate()
            .filter(|(each, _)| *each != slot)
            .map(|(_, stage)| stage.clone())
            .collect();
        let rest = replace_at(steps, at, Some(with_path_stages(branch, path, left)));
        return Some(TakenOut { rest, taken, at });
    }
    if let (Some(lane), Step::Group(group)) = (place.lane, step) {
        let taken = Step::Stage(group.parallel.get(lane)?.clone());
        let left = group
            .parallel
            .iter()
            .enumerate()
            .filter(|(each, _)| *each != lane)
            .map(|(_, stage)| stage.clone())
            .collect();
        let rest = replace_at(steps, at, Some(regroup(group, left)));
        return Some(TakenOut { rest, taken, at });
    }
    Some(TakenOut {
        rest: replace_at(steps, at, None),
        taken: step.clone(),
        at,
    })
}

/// `index + direction` when it stays at or above 0.
fn shift(index: usize, direction: i32) -> Option<usize> {
    index.checked_add_signed(direction as isize)
}

/// One place up (-1) or down (1). A stage in a group moves among its lanes, and leaves the group
/// at its edge. Gives the same steps back when nothing can move.
pub fn move_stage(steps: &[Step], id: &str, direction: i32) -> Vec<Step> {
    let same = || steps.to_vec();
    let Some(place) = find_place(steps, id) else {
        return same();
    };
    let Some(step) = steps.get(place.index) else {
        return same();
    };
    if let (Some(path), Some(slot), Step::Branch(branch)) = (place.path, place.slot, step) {
        return move_path_stage(steps, id, place.index, branch, (path, slot), direction);
    }
    if let (Some(lane), Step::Group(group)) = (place.lane, step) {
        return move_lane(steps, id, place, group, lane, direction);
    }
    let Some(other) = shift(place.index, direction).and_then(|target| steps.get(target)) else {
        return same();
    };
    // A stage next to a branch goes into its nearest path; it leaves by the same controls.
    if let (Step::Branch(next_branch), Step::Stage(stage)) = (other, step)
        && fits_in_path(step)
    {
        let Some(out) = take_out(steps, id) else {
            return same();
        };
        let Some(at) = out.rest.iter().position(|item| item.id() == next_branch.id) else {
            return same();
        };
        let Some(Step::Branch(branch)) = out.rest.get(at) else {
            return same();
        };
        let path = if direction > 0 {
            0
        } else {
            branch.paths.len() - 1
        };
        let held = branch
            .paths
            .get(path)
            .map(|item| item.stages.clone())
            .unwrap_or_default();
        let stages = if direction > 0 {
            [vec![stage.clone()], held].concat()
        } else {
            [held, vec![stage.clone()]].concat()
        };
        return replace_at(&out.rest, at, Some(with_path_stages(branch, path, stages)));
    }
    let Some(target) = shift(place.index, direction) else {
        return same();
    };
    let mut next = steps.to_vec();
    next.swap(place.index, target);
    next
}

fn move_lane(
    steps: &[Step],
    id: &str,
    place: Place,
    group: &StageGroup,
    lane: usize,
    direction: i32,
) -> Vec<Step> {
    if let Some(target) = shift(lane, direction).filter(|target| *target < group.parallel.len()) {
        let mut parallel = group.parallel.clone();
        parallel.swap(lane, target);
        let moved = Step::Group(StageGroup {
            parallel,
            ..group.clone()
        });
        return replace_at(steps, place.index, Some(moved));
    }
    let Some(out) = take_out(steps, id) else {
        return steps.to_vec();
    };
    let at = if direction < 0 { out.at } else { out.at + 1 };
    insert_at(&out.rest, at, out.taken)
}

/// A stage of a path moves among the stages of its path, then into the next path, and out of the
/// branch at the last one.
fn move_path_stage(
    steps: &[Step],
    id: &str,
    index: usize,
    branch: &StageBranch,
    (path, slot): (usize, usize),
    direction: i32,
) -> Vec<Step> {
    let mut stages = branch
        .paths
        .get(path)
        .map(|item| item.stages.clone())
        .unwrap_or_default();
    if let Some(target) = shift(slot, direction).filter(|target| *target < stages.len()) {
        stages.swap(slot, target);
        return replace_at(steps, index, Some(with_path_stages(branch, path, stages)));
    }
    let Some(out) = take_out(steps, id) else {
        return steps.to_vec();
    };
    let Step::Stage(stage) = out.taken.clone() else {
        return steps.to_vec();
    };
    if let Some(Step::Branch(left)) = out.rest.get(out.at)
        && let Some(next) = shift(path, direction).filter(|next| *next < left.paths.len())
    {
        let held = left.paths[next].stages.clone();
        let joined = if direction < 0 {
            [held, vec![stage]].concat()
        } else {
            [vec![stage], held].concat()
        };
        return replace_at(
            &out.rest,
            out.at,
            Some(with_path_stages(left, next, joined)),
        );
    }
    let at = if direction < 0 { out.at } else { out.at + 1 };
    insert_at(&out.rest, at, Step::Stage(stage))
}

pub fn can_move(steps: &[Step], id: &str, direction: i32) -> bool {
    move_stage(steps, id, direction) != steps
}

/// Where a dropped stage goes, next to the target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Before,
    After,
}

/// Drag and drop: puts a stage or a group before or after another. Dropped on a stage of a group,
/// a stage that may run side by side joins that group when it has room.
pub fn drop_stage(steps: &[Step], id: &str, target_id: &str, side: Side) -> Vec<Step> {
    let same = || steps.to_vec();
    if id == target_id {
        return same();
    }
    let from = find_place(steps, id);
    // Dropped on a path that has no stage to drop beside: the stage goes to the end of the path.
    if from.is_some()
        && let Some(into) = find_path(steps, target_id)
    {
        return drop_into_path(steps, id, into);
    }
    let (Some(from), Some(to)) = (from, find_place(steps, target_id)) else {
        return same();
    };
    // Within one group: only the lanes change.
    if let (Some(from_lane), Some(_), Some(Step::Group(group))) =
        (from.lane, to.lane, steps.get(from.index))
        && from.index == to.index
    {
        let mut parallel = group.parallel.clone();
        let moving = parallel.remove(from_lane);
        let Some(target) = parallel.iter().position(|stage| stage.id == target_id) else {
            return same();
        };
        parallel.insert(
            if side == Side::Before {
                target
            } else {
                target + 1
            },
            moving,
        );
        let moved = Step::Group(StageGroup {
            parallel,
            ..group.clone()
        });
        return replace_at(steps, from.index, Some(moved));
    }
    let Some(out) = take_out(steps, id) else {
        return same();
    };
    let Some(place) = find_place(&out.rest, target_id) else {
        return same();
    };
    let Some(host) = out.rest.get(place.index) else {
        return same();
    };
    drop_beside(&out, host, place, side).unwrap_or_else(same)
}

/// The steps with the taken stage next to the target, or `None` to leave the steps as they were.
fn drop_beside(out: &TakenOut, host: &Step, place: Place, side: Side) -> Option<Vec<Step>> {
    let before = side == Side::Before;
    if let (Some(path), Some(slot), Step::Branch(branch)) = (place.path, place.slot, host) {
        let Step::Stage(stage) = &out.taken else {
            return None;
        };
        if !stage_fits_in_path(stage) {
            return None;
        }
        let mut stages = branch.paths.get(path)?.stages.clone();
        stages.insert(if before { slot } else { slot + 1 }, stage.clone());
        return Some(replace_at(
            &out.rest,
            place.index,
            Some(with_path_stages(branch, path, stages)),
        ));
    }
    if let (Some(lane), Step::Group(group), Step::Stage(stage)) = (place.lane, host, &out.taken)
        && runs_in_group(stage.kind)
        && group.parallel.len() < MAX_GROUP
    {
        let mut parallel = group.parallel.clone();
        parallel.insert(if before { lane } else { lane + 1 }, stage.clone());
        let joined = Step::Group(StageGroup {
            parallel,
            ..group.clone()
        });
        return Some(replace_at(&out.rest, place.index, Some(joined)));
    }
    Some(insert_at(
        &out.rest,
        if before { place.index } else { place.index + 1 },
        out.taken.clone(),
    ))
}

fn drop_into_path(steps: &[Step], id: &str, into: PathAt) -> Vec<Step> {
    let same = || steps.to_vec();
    let (Some(target), Some(out)) = (steps.get(into.index), take_out(steps, id)) else {
        return same();
    };
    let Step::Stage(stage) = &out.taken else {
        return same();
    };
    if !stage_fits_in_path(stage) {
        return same();
    }
    let Some(at) = out.rest.iter().position(|step| step.id() == target.id()) else {
        return same();
    };
    let Some(Step::Branch(branch)) = out.rest.get(at) else {
        return same();
    };
    let mut stages = branch
        .paths
        .get(into.path)
        .map(|path| path.stages.clone())
        .unwrap_or_default();
    stages.push(stage.clone());
    replace_at(
        &out.rest,
        at,
        Some(with_path_stages(branch, into.path, stages)),
    )
}

/// Why a stage cannot run alongside the stage above, or `None` when it can.
pub fn join_problem(steps: &[Step], id: &str) -> Option<String> {
    let place = find_place(steps, id);
    if place.is_some_and(|place| place.lane.is_some()) {
        return Some("It runs alongside other stages already".to_string());
    }
    if place.is_some_and(|place| place.path.is_some()) {
        return Some("A path holds single stages only".to_string());
    }
    let (Some(place), Some(Step::Stage(stage))) = (place, place.and_then(|p| steps.get(p.index)))
    else {
        return Some("Only a stage can run alongside another".to_string());
    };
    if !runs_in_group(stage.kind) {
        return Some("Only agent and manual stages run side by side".to_string());
    }
    let Some(above) = place.index.checked_sub(1).and_then(|at| steps.get(at)) else {
        return Some("There is no stage above".to_string());
    };
    match above {
        Step::Branch(branch) => Some(format!(
            "{} is a branch; a stage cannot run alongside it",
            branch_name(branch)
        )),
        Step::Group(group) => (group.parallel.len() >= MAX_GROUP)
            .then(|| "A group holds 4 stages at most".to_string()),
        Step::Stage(above) => (!runs_in_group(above.kind)).then(|| {
            format!(
                "{} cannot run side by side with other stages",
                stage_name(above)
            )
        }),
    }
}

/// Makes a stage run alongside the stage above: it joins the group above, or makes a new group of
/// the two.
pub fn join_above(steps: &[Step], id: &str) -> Vec<Step> {
    let same = || steps.to_vec();
    let Some(place) = find_place(steps, id) else {
        return same();
    };
    if join_problem(steps, id).is_some() {
        return same();
    }
    let (Some(Step::Stage(stage)), Some(above)) = (
        steps.get(place.index),
        place.index.checked_sub(1).and_then(|at| steps.get(at)),
    ) else {
        return same();
    };
    let joined = match above {
        Step::Branch(_) => return same(),
        Step::Group(group) => Step::Group(StageGroup {
            parallel: [group.parallel.clone(), vec![stage.clone()]].concat(),
            ..group.clone()
        }),
        Step::Stage(first) => Step::Group(StageGroup {
            id: new_group_id(steps, &first.id),
            name: None,
            parallel: vec![first.clone(), stage.clone()],
        }),
    };
    let rest = replace_at(steps, place.index, None);
    replace_at(&rest, place.index - 1, Some(joined))
}

/// Takes a stage out of its group; it runs right after the group.
pub fn run_alone(steps: &[Step], id: &str) -> Vec<Step> {
    let Some(place) = find_place(steps, id) else {
        return steps.to_vec();
    };
    if place.lane.is_none() {
        return steps.to_vec();
    }
    match take_out(steps, id) {
        Some(out) => insert_at(&out.rest, out.at + 1, out.taken),
        None => steps.to_vec(),
    }
}

pub fn remove_stage(steps: &[Step], id: &str) -> Vec<Step> {
    take_out(steps, id).map_or_else(|| steps.to_vec(), |out| out.rest)
}

#[cfg(test)]
#[path = "editor_moves_tests.rs"]
mod tests;
