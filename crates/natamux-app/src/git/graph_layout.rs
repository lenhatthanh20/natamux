//! The lanes of the History tab (`features/git/graph-layout.ts`) and its summary texts
//! (`GitGraph.tsx`). Pure, so the view only draws what this says.

// The History tab uses these; they were ported first, with their tests.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit};

/// More names than this would push the message out of its row.
pub const MAX_REFS: usize = 2;

/// One row of the graph, for the commit at the same index. Lanes are columns. A row draws three
/// kinds of lines:
/// - through: lanes that pass from the top to the bottom of the row
/// - into: lanes that come from the top and end at this commit (its children)
/// - out: lanes that leave this commit at the bottom (its parents)
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GraphRow {
    pub lane: usize,
    pub through: Vec<usize>,
    pub into: Vec<usize>,
    pub out: Vec<usize>,
}

fn free_lane(lanes: &[Option<&str>]) -> usize {
    lanes
        .iter()
        .position(Option::is_none)
        .unwrap_or(lanes.len())
}

fn set_lane<'a>(lanes: &mut Vec<Option<&'a str>>, at: usize, hash: &'a str) {
    if lanes.len() <= at {
        lanes.resize(at + 1, None);
    }
    lanes[at] = Some(hash);
}

/// The rows, and the number of lanes the widest row needs. Commits must come newest first, each
/// before its parents (`git log --topo-order`).
pub fn layout_graph(commits: &[GraphCommit]) -> (Vec<GraphRow>, usize) {
    // Each lane holds the hash of the commit it is waiting for, or None when it is free.
    let mut lanes: Vec<Option<&str>> = Vec::new();
    let mut rows = Vec::with_capacity(commits.len());
    let mut width = 0;

    for commit in commits {
        let mut into = Vec::new();
        for (index, slot) in lanes.iter_mut().enumerate() {
            if *slot == Some(commit.hash.as_str()) {
                into.push(index);
                *slot = None;
            }
        }
        let lane = into.first().copied().unwrap_or_else(|| free_lane(&lanes));
        let through = (0..lanes.len()).filter(|i| lanes[*i].is_some()).collect();

        let mut out = Vec::with_capacity(commit.parents.len());
        for (position, parent) in commit.parents.iter().enumerate() {
            if let Some(waiting) = lanes.iter().position(|slot| *slot == Some(parent.as_str())) {
                out.push(waiting);
                continue;
            }
            let lane_is_free = lanes.get(lane).is_none_or(Option::is_none);
            let target = if position == 0 && lane_is_free {
                lane
            } else {
                free_lane(&lanes)
            };
            set_lane(&mut lanes, target, parent);
            out.push(target);
        }

        while lanes.last() == Some(&None) {
            lanes.pop();
        }
        let widest_into = into.iter().map(|i| i + 1).max().unwrap_or(0);
        width = width.max(lane + 1).max(widest_into).max(lanes.len());
        rows.push(GraphRow {
            lane,
            through,
            into,
            out,
        });
    }
    (rows, width)
}

/// "3 commits ahead of main, 0 behind." without the link to the uncommitted files.
pub fn ahead_label(graph: &GitGraph) -> String {
    let (Some(base), Some(ahead), Some(behind)) = (&graph.base, graph.ahead, graph.behind) else {
        return "No base branch to compare with.".to_string();
    };
    let commits = if ahead == 1 {
        "1 commit".to_string()
    } else {
        format!("{ahead} commits")
    };
    format!("{commits} ahead of {base}, {behind} behind.")
}

/// The text of the link to the Changes tab.
pub fn uncommitted_label(count: usize) -> String {
    if count == 1 {
        "1 file not committed.".to_string()
    } else {
        format!("{count} files not committed.")
    }
}

/// Said after the summary when the daemon left older commits out.
pub fn truncated_label(shown: usize) -> String {
    format!(" Only the newest {shown} commits are shown.")
}

/// The ref chips of a row, and the text of the "+N" chip when there are more.
pub fn shown_refs(refs: &[String]) -> (&[String], Option<String>) {
    let shown = &refs[..refs.len().min(MAX_REFS)];
    let more = refs.len().saturating_sub(MAX_REFS);
    (shown, (more > 0).then(|| format!("+{more}")))
}

/// The tooltip of the "+N" chip: the names that did not fit.
pub fn hidden_refs_tip(refs: &[String]) -> String {
    refs.iter()
        .skip(MAX_REFS)
        .cloned()
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "graph_layout_tests.rs"]
mod tests;
