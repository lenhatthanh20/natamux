//! The geometry of the lanes of the History tab (`GitGraph.tsx`): where each line and dot of a
//! row goes inside its 28 px row. The view draws these numbers.

use natamux_proto::workspaces::workspace::{GitGraph, GraphCommit};

use super::graph_layout::GraphRow;

pub const ROW_HEIGHT: f32 = 28.;
pub const LANE_WIDTH: f32 = 14.;
pub const MIDDLE: f32 = ROW_HEIGHT / 2.;

pub fn lane_x(lane: usize) -> f32 {
    lane as f32 * LANE_WIDTH + LANE_WIDTH / 2.
}

/// The width of the lane drawing; one lane wide at least, so the text stays in line.
pub fn lanes_width(width: usize) -> f32 {
    width.max(1) as f32 * LANE_WIDTH
}

/// The newest commit is HEAD of a task's graph; `head` names it too.
pub fn is_head(graph: &GitGraph, index: usize, commit: &GraphCommit) -> bool {
    index == 0 || commit.hash == graph.head
}

pub fn dot_radius(head: bool) -> f32 {
    if head { 5. } else { 4. }
}

/// A line from one lane to another. `color_lane` is the lane whose colour it takes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Link {
    pub color_lane: usize,
    pub from: (f32, f32),
    pub to: (f32, f32),
}

impl Link {
    fn new(color_lane: usize, from_lane: usize, from_y: f32, to_lane: usize, to_y: f32) -> Self {
        Link {
            color_lane,
            from: (lane_x(from_lane), from_y),
            to: (lane_x(to_lane), to_y),
        }
    }

    /// The two control points of the soft curve; None for a straight line.
    pub fn controls(&self) -> Option<[(f32, f32); 2]> {
        if self.from.0 == self.to.0 {
            return None;
        }
        let bend = (self.to.1 - self.from.1) / 2.;
        Some([
            (self.from.0, self.from.1 + bend),
            (self.to.0, self.to.1 - bend),
        ])
    }
}

/// The lines of a row: lanes that pass through, lanes that end in the dot, lanes that start at it.
pub fn links(row: &GraphRow) -> Vec<Link> {
    let through = row
        .through
        .iter()
        .map(|&lane| Link::new(lane, lane, 0., lane, ROW_HEIGHT));
    let into = row
        .into
        .iter()
        .map(|&lane| Link::new(lane, lane, 0., row.lane, MIDDLE));
    let out = row
        .out
        .iter()
        .map(|&lane| Link::new(lane, row.lane, MIDDLE, lane, ROW_HEIGHT));
    through.chain(into).chain(out).collect()
}

#[cfg(test)]
#[path = "graph_lanes_tests.rs"]
mod tests;
