//! Edits of branches in the pipeline editor (`pipeline-editor/branches.ts`): adding and removing
//! paths, and their conditions and names.

use natamux_proto::stages::pipeline_rules::condition_problems;
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch, Step};

use crate::stages::editor_draft::MAX_PATHS;
use crate::stages::editor_find::{
    find_branch, find_place, free_id, free_path_id, insert_at, release_index, used_ids,
};

fn update_branch(steps: &[Step], branch_id: &str, change: impl Fn(&mut StageBranch)) -> Vec<Step> {
    let mut next = steps.to_vec();
    for step in &mut next {
        if let Step::Branch(branch) = step
            && branch.id == branch_id
        {
            change(branch);
        }
    }
    next
}

pub fn update_path(steps: &[Step], path_id: &str, change: impl Fn(&mut BranchPath)) -> Vec<Step> {
    let mut next = steps.to_vec();
    for step in &mut next {
        if let Step::Branch(branch) = step
            && branch.paths.iter().any(|path| path.id == path_id)
        {
            for path in branch.paths.iter_mut().filter(|path| path.id == path_id) {
                change(path);
            }
        }
    }
    next
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionType {
    Output,
    Command,
}

/// A condition to start from. It is not valid until the person fills it in.
pub fn blank_condition(kind: ConditionType) -> Condition {
    match kind {
        ConditionType::Command => Condition::Command { run: String::new() },
        ConditionType::Output => Condition::Output {
            file: "plan.md".to_string(),
            section: None,
            contains: String::new(),
        },
    }
}

fn type_of(when: &Condition) -> ConditionType {
    match when {
        Condition::Command { .. } => ConditionType::Command,
        Condition::Output { .. } => ConditionType::Output,
    }
}

/// The condition with another type; the same one when the type does not change.
pub fn retype_condition(when: Option<&Condition>, kind: ConditionType) -> Condition {
    match when {
        Some(same) if type_of(same) == kind => same.clone(),
        _ => blank_condition(kind),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConditionField {
    File,
    Section,
    Contains,
    Run,
}

/// One field of a condition. An empty section is left out.
pub fn edit_condition(when: &Condition, field: ConditionField, value: &str) -> Condition {
    match (when, field) {
        (Condition::Command { .. }, ConditionField::Run) => Condition::Command {
            run: value.to_string(),
        },
        (
            Condition::Output {
                section, contains, ..
            },
            ConditionField::File,
        ) => Condition::Output {
            file: value.to_string(),
            section: section.clone(),
            contains: contains.clone(),
        },
        (Condition::Output { file, section, .. }, ConditionField::Contains) => Condition::Output {
            file: file.clone(),
            section: section.clone(),
            contains: value.to_string(),
        },
        (Condition::Output { file, contains, .. }, ConditionField::Section) => Condition::Output {
            file: file.clone(),
            section: (!value.trim().is_empty()).then(|| value.to_string()),
            contains: contains.clone(),
        },
        _ => when.clone(),
    }
}

/// The messages of the rules a condition breaks, by field. The first message of a field wins.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ConditionProblems {
    pub file: Option<String>,
    pub section: Option<String>,
    pub contains: Option<String>,
    pub run: Option<String>,
}

pub fn condition_problems_by_field(when: &Condition) -> ConditionProblems {
    let mut problems = ConditionProblems::default();
    for (field, message) in condition_problems(when) {
        let slot = match field {
            "file" => &mut problems.file,
            "section" => &mut problems.section,
            "contains" => &mut problems.contains,
            "run" => &mut problems.run,
            _ => continue,
        };
        slot.get_or_insert(message);
    }
    problems
}

/// A new branch goes after the step with this id, or before the release. It starts with a path
/// that has a condition and an Otherwise path, both without stages: the person adds them.
pub fn add_branch(steps: &[Step], after_id: Option<&str>) -> Vec<Step> {
    let place = after_id.and_then(|id| find_place(steps, id));
    let end = release_index(steps);
    let index = place.map_or(end, |place| (place.index + 1).min(end));
    let mut used = used_ids(steps);
    let id = free_id(&used, "branch");
    used.insert(id.clone());
    let first = free_path_id(&used, 1);
    used.insert(first.clone());
    let second = free_path_id(&used, 2);
    let path = |id: String, when| BranchPath {
        id,
        name: None,
        when,
        not: None,
        stages: Vec::new(),
    };
    let branch = StageBranch {
        id,
        name: None,
        paths: vec![
            path(first, Some(blank_condition(ConditionType::Output))),
            path(second, None),
        ],
    };
    insert_at(steps, index, Step::Branch(branch))
}

/// A path with a condition. It goes before the Otherwise path, which stays last. A branch holds 4
/// paths at most.
pub fn add_path(steps: &[Step], branch_id: &str) -> Vec<Step> {
    let Some(branch) = find_branch(steps, branch_id) else {
        return steps.to_vec();
    };
    if branch.paths.len() >= MAX_PATHS {
        return steps.to_vec();
    }
    let id = free_path_id(&used_ids(steps), branch.paths.len() + 1);
    let added = BranchPath {
        id,
        name: None,
        when: Some(blank_condition(ConditionType::Output)),
        not: None,
        stages: Vec::new(),
    };
    update_branch(steps, branch_id, |item| {
        let otherwise_last = item.paths.last().is_some_and(|last| last.when.is_none());
        let at = if otherwise_last {
            item.paths.len() - 1
        } else {
            item.paths.len()
        };
        item.paths.insert(at, added.clone());
    })
}

/// The path goes with its stages. A branch keeps one path at least.
pub fn remove_path(steps: &[Step], branch_id: &str, path_id: &str) -> Vec<Step> {
    match find_branch(steps, branch_id) {
        Some(branch) if branch.paths.len() >= 2 => update_branch(steps, branch_id, |item| {
            item.paths.retain(|path| path.id != path_id)
        }),
        _ => steps.to_vec(),
    }
}

/// "Does not hold" is `not: true`; "holds" leaves the field out.
pub fn set_path_condition(
    steps: &[Step],
    branch_id: &str,
    path_id: &str,
    when: &Condition,
    not: bool,
) -> Vec<Step> {
    if find_branch(steps, branch_id).is_none() {
        return steps.to_vec();
    }
    update_path(steps, path_id, |path| {
        path.when = Some(when.clone());
        path.not = not.then_some(true);
    })
}

/// Only the last path can be the Otherwise path. Turning it off gives it a blank condition.
pub fn set_otherwise(steps: &[Step], branch_id: &str, path_id: &str, on: bool) -> Vec<Step> {
    let last = find_branch(steps, branch_id).and_then(|branch| branch.paths.last());
    match last {
        Some(last) if last.id == path_id && last.when.is_none() != on => {
            update_path(steps, path_id, |path| {
                path.not = None;
                path.when = (!on).then(|| blank_condition(ConditionType::Output));
            })
        }
        _ => steps.to_vec(),
    }
}

/// An empty name removes the field, so the id shows as the name.
pub fn set_branch_name(steps: &[Step], branch_id: &str, name: &str) -> Vec<Step> {
    update_branch(steps, branch_id, |branch| {
        branch.name = (!name.trim().is_empty()).then(|| name.to_string());
    })
}

pub fn set_path_name(steps: &[Step], branch_id: &str, path_id: &str, name: &str) -> Vec<Step> {
    if find_branch(steps, branch_id).is_none() {
        return steps.to_vec();
    }
    update_path(steps, path_id, |path| {
        path.name = (!name.trim().is_empty()).then(|| name.to_string());
    })
}

#[cfg(test)]
#[path = "editor_branches_tests.rs"]
mod tests;
