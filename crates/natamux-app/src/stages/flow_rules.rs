//! Pure rules of the Progress node flow (T-0246): the style of a connector, the tone of a node,
//! the keyword and state of a branch lane, the branch header line, the layout of a group and
//! where the two nodes of a pair have their ports. No colours here; `flow_lines.rs` maps them.

// The Progress view calls them; this allow keeps a build green while it is wired in parts.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{BranchPath, StageBranch};
use natamux_proto::tasks::task::TaskState;

use super::stage_view_rules::{ProgressLookup, branch_status, path_label, taken_path};

/// Sizes in px at the base rem size (16 px); views turn them into rems.
pub const PORT: f32 = 6.;
/// How far a port pokes out past the outer edge of a node with a 1 px border: its centre sits on
/// the middle of the border.
pub const PORT_OUT: f32 = PORT / 2. - 0.5;
/// The height of a straight connector between two steps.
pub const LINK_H: f32 = 16.;
/// The height of the fork and the join curves of a pair.
pub const CURVE_H: f32 = 20.;
/// The gap between the two nodes of a pair.
pub const PAIR_GAP: f32 = 8.;
pub const JOIN_DOT: f32 = 8.;
/// Width of every connector stroke, in px (a hairline, not scaled).
pub const STROKE: f32 = 1.5;
/// Distance between the dots of the canvas grid.
pub const GRID: f32 = 16.;

/// How a connector is drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkStyle {
    /// Solid, success colour.
    Done,
    /// Solid, in the colour of the state of the stage it leads into.
    Current(TaskState),
    /// Dashed, muted.
    Upcoming,
    /// Dotted, faint.
    NotTaken,
    /// Dashed, faint.
    Skipped,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stroke {
    Solid,
    Dashed,
    Dotted,
}

impl LinkStyle {
    pub fn stroke(self) -> Stroke {
        match self {
            LinkStyle::Done | LinkStyle::Current(_) => Stroke::Solid,
            LinkStyle::Upcoming | LinkStyle::Skipped => Stroke::Dashed,
            LinkStyle::NotTaken => Stroke::Dotted,
        }
    }
}

/// The connector from one step to the next. A group or a branch passes its summed status
/// (`group_status`, `branch_status`); `after_state` is the task state when `after` is current.
pub fn link_style(
    before: StageStatus,
    after: StageStatus,
    after_state: Option<TaskState>,
) -> LinkStyle {
    if before == StageStatus::NotTaken {
        return LinkStyle::NotTaken;
    }
    match after {
        StageStatus::Done => LinkStyle::Done,
        StageStatus::Current => LinkStyle::Current(after_state.unwrap_or(TaskState::Ready)),
        StageStatus::Upcoming => LinkStyle::Upcoming,
        StageStatus::Skipped => LinkStyle::Skipped,
        StageStatus::NotTaken => LinkStyle::NotTaken,
    }
}

/// A join curve and the join dot: done only once the stage (or the whole group) is finished, so a
/// running stage's way out stays dashed.
pub fn join_style(status: StageStatus) -> LinkStyle {
    match status {
        StageStatus::Done => LinkStyle::Done,
        StageStatus::Skipped => LinkStyle::Skipped,
        StageStatus::NotTaken => LinkStyle::NotTaken,
        StageStatus::Current | StageStatus::Upcoming => LinkStyle::Upcoming,
    }
}

/// The status of a group from its stages: current while one is, done when all are finished.
pub fn group_status(statuses: &[StageStatus]) -> StageStatus {
    if statuses.is_empty() {
        return StageStatus::Upcoming;
    }
    let all = |pick: fn(&StageStatus) -> bool| statuses.iter().all(pick);
    if statuses.contains(&StageStatus::Current) {
        StageStatus::Current
    } else if all(|s| *s == StageStatus::NotTaken) {
        StageStatus::NotTaken
    } else if all(|s| *s == StageStatus::Skipped) {
        StageStatus::Skipped
    } else if all(|s| matches!(*s, StageStatus::Done | StageStatus::Skipped)) {
        StageStatus::Done
    } else {
        StageStatus::Upcoming
    }
}

/// The look of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeTone {
    Done,
    Current {
        state: TaskState,
        /// Its CI check runs now (a CI branch is known for it).
        checking: bool,
    },
    Upcoming,
    /// Upcoming and marked to skip ("Don't skip").
    WillSkip,
    Skipped,
    NotTaken,
}

pub fn node_tone(
    status: StageStatus,
    state: Option<TaskState>,
    will_skip: bool,
    checking: bool,
) -> NodeTone {
    match status {
        StageStatus::Done => NodeTone::Done,
        StageStatus::Current => NodeTone::Current {
            state: state.unwrap_or(TaskState::Ready),
            checking,
        },
        StageStatus::Upcoming if will_skip => NodeTone::WillSkip,
        StageStatus::Upcoming => NodeTone::Upcoming,
        StageStatus::Skipped => NodeTone::Skipped,
        StageStatus::NotTaken => NodeTone::NotTaken,
    }
}

impl NodeTone {
    pub fn border(self) -> Stroke {
        match self {
            NodeTone::Upcoming | NodeTone::WillSkip => Stroke::Dashed,
            NodeTone::NotTaken => Stroke::Dotted,
            NodeTone::Done | NodeTone::Current { .. } | NodeTone::Skipped => Stroke::Solid,
        }
    }

    /// Muted text, no fill.
    pub fn muted(self) -> bool {
        matches!(
            self,
            NodeTone::Upcoming | NodeTone::WillSkip | NodeTone::Skipped | NodeTone::NotTaken
        )
    }

    /// State-coloured border and the still glow ring.
    pub fn glow(self) -> bool {
        matches!(self, NodeTone::Current { .. })
    }

    /// The name is struck through.
    pub fn struck(self) -> bool {
        self == NodeTone::Skipped
    }

    /// The state whose colour the tile and the border take; None is muted.
    pub fn color_state(self) -> Option<TaskState> {
        match self {
            NodeTone::Done => Some(TaskState::Done),
            NodeTone::Current { state, .. } => Some(state),
            _ => None,
        }
    }

    /// The state of the `StateGlyph` on the tile corner; None draws no overlay. A checking node
    /// shows the CI mark instead (the caller's choice of icon).
    pub fn glyph_state(self) -> Option<TaskState> {
        match self {
            NodeTone::Done => Some(TaskState::Done),
            NodeTone::Current { state, .. } => Some(state),
            NodeTone::Skipped => Some(TaskState::Cancelled),
            NodeTone::Upcoming | NodeTone::WillSkip | NodeTone::NotTaken => None,
        }
    }
}

/// The word before a lane's name. `not` reads "unless" at any place, as the design draws it; a
/// path with no condition is the Otherwise path.
pub fn lane_keyword(index: usize, path: &BranchPath) -> &'static str {
    match (&path.when, path.not == Some(true), index) {
        (None, _, _) => "otherwise",
        (Some(_), true, _) => "unless",
        (Some(_), false, 0) => "if",
        (Some(_), false, _) => "else if",
    }
}

/// Where a lane is: before the branch is reached every lane is open with upcoming stages.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaneState {
    NotReached,
    Taken,
    NotTaken,
}

pub fn lane_state(
    branch: &StageBranch,
    path: &BranchPath,
    status_of: &ProgressLookup,
) -> LaneState {
    match taken_path(branch, status_of) {
        Some(taken) if taken.id == path.id => LaneState::Taken,
        Some(_) => LaneState::NotTaken,
        None if branch_status(branch, status_of) != StageStatus::Upcoming => LaneState::NotTaken,
        None => LaneState::NotReached,
    }
}

/// The one line under "If · <name>".
pub fn branch_line(branch: &StageBranch, status_of: &ProgressLookup) -> String {
    if let Some(path) = taken_path(branch, status_of) {
        return format!("Taken: {}", path_label(path));
    }
    if branch_status(branch, status_of) == StageStatus::Upcoming {
        "Picks a path when it gets here".to_owned()
    } else {
        "No path was taken".to_owned()
    }
}

pub fn stage_count_label(count: usize) -> String {
    if count == 1 {
        "1 stage".to_owned()
    } else {
        format!("{count} stages")
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GroupLayout {
    /// Two half-width nodes with fork and join curves.
    Pair,
    /// A dashed frame with full-width compact nodes, one under the other.
    Stack,
}

pub fn group_layout(count: usize) -> GroupLayout {
    if count == 2 {
        GroupLayout::Pair
    } else {
        GroupLayout::Stack
    }
}

/// The x of the two ports of a pair, from the left of a row `width` wide whose two `flex_1` nodes
/// sit `gap` apart. Each node is `(width - gap) / 2` wide.
pub fn pair_centres(width: f32, gap: f32) -> (f32, f32) {
    let half = (width - gap).max(0.) / 4.;
    (half, width - half)
}

#[cfg(test)]
#[path = "flow_rules_tests.rs"]
mod tests;
