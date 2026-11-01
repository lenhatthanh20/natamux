//! The flow around the nodes of the Progress tab (T-0246): what a step knows of its neighbours,
//! the ports a node gets, and one column of steps joined by connectors. A path of a branch uses
//! the same column, so a path that holds a group or a branch needs no new code.

use gpui::{AnyElement, Div};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::Ctx;
use crate::stages::flow_lines::FlowLink;
use crate::stages::flow_rules::{
    CaretAt, LinkStyle, NodeLinks, group_status, link_style,
};
use crate::stages::stage_view_rules::branch_status;

/// The status of a step, and the state of its current stage.
#[derive(Clone, Copy)]
pub(super) struct Edge {
    pub status: StageStatus,
    pub state: Option<TaskState>,
}

/// The steps next to one step in its column: none before the first, none after the last.
#[derive(Clone, Copy, Default)]
pub(super) struct Around {
    pub prev: Option<StageStatus>,
    pub next: Option<Edge>,
}

impl Around {
    /// The connector out of a step; none for the last one.
    pub fn out_style(self, own: Edge) -> Option<LinkStyle> {
        self.next
            .map(|next| link_style(own.status, next.status, next.state))
    }

    /// The connector into a step; none for the first one.
    pub fn in_style(self, own: Edge) -> Option<LinkStyle> {
        self.prev
            .map(|prev| link_style(prev, own.status, own.state))
    }
}

/// The box of a node in the flow. It grows, so a node stretched by a pair row fills it. The
/// lines meet the node's box with no port ring (owner, T-0257).
pub(super) fn ported(node: AnyElement) -> Div {
    v_flex().child(v_flex().flex_grow_1().child(node))
}

impl Ctx<'_> {
    pub(super) fn stage_edge(&self, id: &str) -> Edge {
        let entry = self.by_id.get(id);
        Edge {
            status: entry.map_or(StageStatus::Upcoming, |entry| entry.status),
            state: entry.and_then(|entry| entry.state),
        }
    }

    pub(super) fn edge(&self, step: &Step) -> Edge {
        let lookup = |id: &str| self.by_id.get(id).map(|entry| entry.status);
        let own = self.entries(step);
        let status = match step {
            Step::Branch(branch) => branch_status(branch, &lookup),
            _ => {
                let statuses: Vec<StageStatus> = own.iter().map(|entry| entry.status).collect();
                group_status(&statuses)
            }
        };
        let state = own
            .iter()
            .find(|entry| entry.status == StageStatus::Current)
            .and_then(|entry| entry.state);
        Edge { status, state }
    }

    /// One step with the connector under it; the elements of a column are one per step.
    /// `in_path`: a step of a branch's path, whose details card goes under the whole branch.
    fn flow_step(&self, step: &Step, around: Around, in_path: bool, cx: &App) -> AnyElement {
        let own = self.edge(step);
        match step {
            Step::Stage(stage) => {
                let out = around.out_style(own);
                // The selected node's card sits right under it; the line goes on from the card.
                let details = (!in_path)
                    .then(|| self.details_of([stage], CaretAt::Middle, cx))
                    .flatten();
                let open = details.is_some();
                let links = NodeLinks {
                    into: around.in_style(own),
                    out: out.filter(|_| !open),
                };
                let card = self
                    .card(&stage.id, links, cx)
                    .unwrap_or_else(|| div().into_any_element());
                v_flex()
                    .w_full()
                    .child(ported(card))
                    .children(details)
                    .children(out.map(FlowLink::new))
                    .into_any_element()
            }
            Step::Group(group) => self.group(group, own, around, cx),
            Step::Branch(branch) => self.branch(branch, own, around, cx),
        }
    }

    /// The steps top to bottom, joined by connectors. The first has no port above and the last
    /// none below.
    pub(super) fn column(&self, steps: &[Step], in_path: bool, cx: &App) -> Vec<AnyElement> {
        let edges: Vec<Edge> = steps.iter().map(|step| self.edge(step)).collect();
        steps
            .iter()
            .enumerate()
            .map(|(index, step)| {
                let around = Around {
                    prev: index
                        .checked_sub(1)
                        .and_then(|before| edges.get(before))
                        .map(|edge| edge.status),
                    next: edges.get(index + 1).copied(),
                };
                self.flow_step(step, around, in_path, cx)
            })
            .collect()
    }
}
