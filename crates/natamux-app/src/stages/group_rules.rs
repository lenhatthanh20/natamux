//! The rules of the Side by side dialog: which stages can make a group, and making, changing and
//! splitting a group. A group lives in one place, the top level or one path of a branch (T-0283),
//! so the stages of one group must all come from that place.

use natamux_proto::stages::schema::{Stage, StageGroup, Step};

use crate::stages::editor_draft::MAX_GROUP;
use crate::stages::editor_find::{find_place, new_group_id, runs_in_group};
use crate::stages::editor_labels::path_name;
use crate::stages::editor_moves::in_path;

/// Where a stage of the dialog's checklist lives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GroupPlace {
    Top,
    Path { id: String, name: String },
}

/// The agent and manual stages that are not in a group, with their place, in list order. The
/// members of `keep` (the group being edited) stay in the list.
pub fn group_choices(steps: &[Step], keep: Option<&str>) -> Vec<(GroupPlace, Stage)> {
    let mut choices = Vec::new();
    for step in steps {
        if let Step::Branch(branch) = step {
            for path in &branch.paths {
                let place = GroupPlace::Path {
                    id: path.id.clone(),
                    name: path_name(path),
                };
                choices.extend(
                    choices_in(&path.stages, keep)
                        .into_iter()
                        .map(|stage| (place.clone(), stage)),
                );
            }
        } else {
            choices.extend(
                choices_in(std::slice::from_ref(step), keep)
                    .into_iter()
                    .map(|stage| (GroupPlace::Top, stage)),
            );
        }
    }
    choices
}

fn choices_in(items: &[Step], keep: Option<&str>) -> Vec<Stage> {
    items
        .iter()
        .flat_map(|item| match item {
            Step::Stage(stage) if runs_in_group(stage.kind) => vec![stage.clone()],
            Step::Group(group) if Some(group.id.as_str()) == keep => group.parallel.clone(),
            _ => Vec::new(),
        })
        .collect()
}

/// The list a stage is in: None for the top level, else its branch and path.
fn list_of(steps: &[Step], id: &str) -> Option<Option<(usize, usize)>> {
    let place = find_place(steps, id)?;
    Some(place.path.map(|path| (place.index, path)))
}

/// All the stages are in one list: the top level, or one path.
pub fn same_place(steps: &[Step], ids: &[String]) -> bool {
    let mut lists = ids.iter().map(|id| list_of(steps, id));
    match lists.next() {
        None => true,
        Some(first) => first.is_some() && lists.all(|list| list == first),
    }
}

/// Runs `edit` on the list that holds `id` (the top level, or the items of its path). Steps come
/// back equal when the id is unknown or `edit` refuses.
fn in_list_of(
    steps: &[Step],
    id: &str,
    edit: impl FnOnce(&[Step]) -> Option<Vec<Step>>,
) -> Vec<Step> {
    let same = || steps.to_vec();
    let Some(place) = find_place(steps, id) else {
        return same();
    };
    if place.path.is_none() {
        return edit(steps).unwrap_or_else(same);
    }
    let edited = in_path(steps, place, |items| {
        edit(items).unwrap_or_else(|| items.to_vec())
    });
    edited.unwrap_or_else(same)
}

fn name_of(name: &str) -> Option<String> {
    (!name.trim().is_empty()).then(|| name.to_string())
}

/// A group holds 2 to 4 stages, and no stage twice.
fn fits_group(ids: &[String]) -> bool {
    let unique = ids
        .iter()
        .enumerate()
        .all(|(at, id)| !ids[..at].contains(id));
    unique && (2..=MAX_GROUP).contains(&ids.len())
}

/// The stages of `ids` that stand on their own in `items` and may run side by side, with their
/// index, in list order.
fn loose_stages(items: &[Step], ids: &[String]) -> Vec<(usize, Stage)> {
    items
        .iter()
        .enumerate()
        .filter_map(|(at, item)| match item {
            Step::Stage(stage) if ids.contains(&stage.id) && runs_in_group(stage.kind) => {
                Some((at, stage.clone()))
            }
            _ => None,
        })
        .collect()
}

/// The stages of `ids` become one group where the first of them (in list order) is, in list order.
/// An empty name leaves the group without one. Refused unless 2 to 4 agent or manual stages of
/// one place, none of them in a group.
pub fn make_group(steps: &[Step], ids: &[String], name: &str) -> Vec<Step> {
    let Some(first) = ids.first() else {
        return steps.to_vec();
    };
    if !fits_group(ids) || !same_place(steps, ids) {
        return steps.to_vec();
    }
    in_list_of(steps, first, |items| {
        let picked = loose_stages(items, ids);
        let (at, lead) = picked.first()?;
        if picked.len() != ids.len() {
            return None;
        }
        let group = Step::Group(StageGroup {
            id: new_group_id(steps, &lead.id),
            name: name_of(name),
            parallel: picked.iter().map(|(_, stage)| stage.clone()).collect(),
        });
        let mut next = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if index == *at {
                next.push(group.clone());
            } else if !picked.iter().any(|(taken, _)| *taken == index) {
                next.push(item.clone());
            }
        }
        Some(next)
    })
}

/// The group keeps its id and place. Members left out run on their own right after it, in order;
/// added stages (from the same place) join at the end, in list order.
pub fn edit_group(steps: &[Step], group_id: &str, ids: &[String], name: &str) -> Vec<Step> {
    if !fits_group(ids) {
        return steps.to_vec();
    }
    in_list_of(steps, group_id, |items| {
        let at = items.iter().position(|item| item.id() == group_id)?;
        let Step::Group(group) = items.get(at)? else {
            return None;
        };
        let (kept, left): (Vec<Stage>, Vec<Stage>) = group
            .parallel
            .iter()
            .cloned()
            .partition(|stage| ids.contains(&stage.id));
        let added = loose_stages(items, ids);
        if kept.len() + added.len() != ids.len() {
            return None;
        }
        let edited = Step::Group(StageGroup {
            id: group.id.clone(),
            name: name_of(name),
            parallel: [kept, added.iter().map(|(_, stage)| stage.clone()).collect()].concat(),
        });
        let mut next = Vec::new();
        for (index, item) in items.iter().enumerate() {
            if index == at {
                next.push(edited.clone());
                next.extend(left.iter().cloned().map(Step::Stage));
            } else if !added.iter().any(|(taken, _)| *taken == index) {
                next.push(item.clone());
            }
        }
        Some(next)
    })
}

/// Every member runs on its own, in order, where the group was.
pub fn split_group(steps: &[Step], group_id: &str) -> Vec<Step> {
    in_list_of(steps, group_id, |items| {
        let at = items.iter().position(|item| item.id() == group_id)?;
        let Step::Group(group) = items.get(at)? else {
            return None;
        };
        let mut next = items[..at].to_vec();
        next.extend(group.parallel.iter().cloned().map(Step::Stage));
        next.extend(items[at + 1..].iter().cloned());
        Some(next)
    })
}

#[cfg(test)]
#[path = "group_rules_tests.rs"]
mod tests;
