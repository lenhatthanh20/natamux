//! The flow around the nodes of the Progress tab (T-0246): what a step knows of its neighbours,
//! the ports a node gets, and one column of steps joined by connectors. A path of a branch uses
//! the same column, so a path that holds a group or a branch needs no new code.

use gpui::{AnyElement, Div, px};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::Step;
use natamux_proto::tasks::task::TaskState;
use ui::prelude::*;

use super::Ctx;
use crate::stages::flow_lines::{FlowLink, Port};
use crate::stages::flow_rules::{LinkStyle, group_status, link_style};
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
    /// The connector into a step; none for the first one.
    pub fn into_style(self, own: Edge) -> Option<LinkStyle> {
        self.prev
            .map(|prev| link_style(prev, own.status, own.state))
    }

    /// The connector out of a step; none for the last one.
    pub fn out_style(self, own: Edge) -> Option<LinkStyle> {
        self.next
            .map(|next| link_style(own.status, next.status, next.state))
    }
}

/// A node with its ports. The node draws none (contract with `stage_card`), so the box here has
/// the 1 px border the ports are placed against, and the node sits over that border. The inner
/// box grows, so a node stretched by a pair row fills it and its bottom port stays on its edge.
pub(super) fn ported(node: AnyElement, top: Option<LinkStyle>, bottom: Option<LinkStyle>) -> Div {
    v_flex()
        .relative()
        .border_1()
        .border_color(gpui::transparent_black())
        .child(v_flex().flex_grow_1().m(px(-1.)).child(node))
        .children(top.map(Port::top))
        .children(bottom.map(Port::bottom))
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
    fn flow_step(&self, step: &Step, around: Around, cx: &App) -> AnyElement {
        let own = self.edge(step);
        match step {
            Step::Stage(stage) => {
                let out = around.out_style(own);
                let card = self
                    .card(&stage.id, false, cx)
                    .unwrap_or_else(|| div().into_any_element());
                v_flex()
                    .w_full()
                    .child(ported(card, around.into_style(own), out))
                    .children(out.map(FlowLink::new))
                    .into_any_element()
            }
            Step::Group(group) => self.group(group, own, around, cx),
            Step::Branch(branch) => self.branch(branch, own, around, cx),
        }
    }

    /// The details card of the selected node when stage or branch step `index` holds it (T-0257).
    /// The step's out connector leads into the card, and a connector of the same style leads on
    /// to the next step, so the line is not broken. A path's column does not call it: the card
    /// goes under the whole branch frame.
    pub(super) fn details_below(
        &self,
        steps: &[Step],
        index: usize,
        cx: &App,
    ) -> Option<AnyElement> {
        let selected = self.input.selected.as_deref()?;
        let step = steps.get(index)?;
        // A group puts the card between its nodes and its join dot itself.
        if matches!(step, Step::Group(_)) || !step.stages().iter().any(|stage| stage.id == selected)
        {
            return None;
        }
        let card = self.details(selected, cx)?;
        let own = self.edge(step);
        let out = steps.get(index + 1).map(|next| {
            let next = self.edge(next);
            link_style(own.status, next.status, next.state)
        });
        Some(match out {
            Some(style) => v_flex()
                .w_full()
                .child(ported(card, Some(style), Some(style)))
                .child(FlowLink::new(style))
                .into_any_element(),
            None => v_flex().w_full().pt_2().child(card).into_any_element(),
        })
    }

    /// The steps top to bottom, joined by connectors. The first has no port above and the last
    /// none below.
    pub(super) fn column(&self, steps: &[Step], cx: &App) -> Vec<AnyElement> {
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
                self.flow_step(step, around, cx)
            })
            .collect()
    }
}
