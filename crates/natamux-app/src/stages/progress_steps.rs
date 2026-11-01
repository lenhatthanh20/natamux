//! The group row of the Progress tab: "SIDE BY SIDE" on the line, then its stages side by side
//! (two: fork and join curves; three or four: a dashed frame) and the join dot with the note
//! ("Review waits for both"). The branch is in `progress_branch.rs`.

use gpui::{AnyElement, FontWeight};
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{Stage, StageGroup, Step};
use ui::{Tooltip, prelude::*};

use super::Ctx;
use super::flow::{Around, Edge, ported};
use crate::stages::flow_lines::{FlowLink, JoinDot, PairCurves};
use crate::stages::flow_rules::{
    CaretAt, GroupLayout, LinkStyle, NodeLinks, NodeSide, PAIR_GAP, group_layout, join_style,
    link_style,
};

impl Ctx<'_> {
    /// What the join dot says: which step starts when the group is finished.
    fn group_note(&self, group: &StageGroup) -> String {
        let steps = &self.input.steps;
        let index = steps
            .iter()
            .position(|step| step.id() == group.id)
            .unwrap_or(0);
        match steps.get(index + 1) {
            None => "The task is done when both are finished".to_owned(),
            Some(Step::Branch(_)) => "A path is chosen when both are finished".to_owned(),
            Some(next) => {
                let first = next
                    .stages()
                    .first()
                    .map(|stage| stage.id.clone())
                    .unwrap_or_default();
                let name = self
                    .by_id
                    .get(first.as_str())
                    .map_or(first.clone(), |entry| entry.name.clone());
                format!("{name} waits for both")
            }
        }
    }

    pub(super) fn group(
        &self,
        group: &StageGroup,
        own: Edge,
        around: Around,
        cx: &App,
    ) -> AnyElement {
        let name = group.name.clone().unwrap_or_else(|| {
            group
                .parallel
                .iter()
                .map(|stage| stage.id.as_str())
                .collect::<Vec<_>>()
                .join(" and ")
        });
        let prev = around.prev.unwrap_or(StageStatus::Upcoming);
        let into = link_style(prev, own.status, own.state);
        let body = match (
            group_layout(group.parallel.len()),
            group.parallel.as_slice(),
        ) {
            (GroupLayout::Pair, [first, second]) => self.pair(first, second, prev, cx),
            _ => self.stack(group, own, into, cx),
        };
        let dot = JoinDot::new(join_style(own.status)).note(self.group_note(group));
        let dot = match around.out_style(own) {
            Some(below) => dot.below(below),
            None => dot,
        };
        v_flex()
            .w_full()
            .child(side_label(&group.id, &name))
            .child(body)
            .child(dot)
            .into_any_element()
    }

    /// Two nodes side by side, between the fork and the join curves, their tiles near the middle
    /// and their texts outside (T-0259). The selected node's card sits right under the pair, its
    /// caret at that node, and the join curves start from the card (mockup A of T-0257).
    fn pair(&self, first: &Stage, second: &Stage, prev: StageStatus, cx: &App) -> AnyElement {
        let details = self
            .details_of([first], CaretAt::Left, cx)
            .or_else(|| self.details_of([second], CaretAt::Right, cx));
        let open = details.is_some();
        let side = |stage: &Stage, at: NodeSide| {
            let edge = self.stage_edge(&stage.id);
            let into = link_style(prev, edge.status, edge.state);
            let links = NodeLinks {
                into: Some(into),
                out: (!open).then(|| join_style(edge.status)),
            };
            let card = self
                .card(&stage.id, at, links, cx)
                .unwrap_or_else(|| div().into_any_element());
            let node = ported(card).flex_1().min_w_0();
            (edge, into, node)
        };
        let (left, left_into, left_node) = side(first, NodeSide::Left);
        let (right, right_into, right_node) = side(second, NodeSide::Right);
        let join = PairCurves::join(join_style(left.status), join_style(right.status));
        v_flex()
            .w_full()
            .child(PairCurves::fork(left_into, right_into))
            .child(
                // Stretched: both nodes take the taller one's height, so the join curves start
                // at the same bottom line.
                h_flex()
                    .w_full()
                    .items_stretch()
                    .gap(rems_from_px(PAIR_GAP))
                    .child(left_node)
                    .child(right_node),
            )
            .children(details)
            .child(join)
            .into_any_element()
    }

    /// Three or four nodes, one under the other, in a dashed frame that carries the ports. The
    /// selected node's card sits right under the frame.
    fn stack(&self, group: &StageGroup, own: Edge, into: LinkStyle, cx: &App) -> AnyElement {
        let out = join_style(own.status);
        let details = self.details_of(&group.parallel, CaretAt::Middle, cx);
        let frame = v_flex()
            .relative()
            .w_full()
            .p_1()
            .gap_1()
            .rounded_md()
            .border_1()
            .border_dashed()
            .border_color(cx.theme().colors().border)
            .children(
                group
                    .parallel
                    .iter()
                    .filter_map(|stage| {
                        self.card(&stage.id, NodeSide::Middle, NodeLinks::default(), cx)
                    }),
            );
        v_flex()
            .w_full()
            .child(FlowLink::new(into))
            .child(frame)
            .children(details)
            .child(FlowLink::new(out))
            .into_any_element()
    }
}

/// "SIDE BY SIDE" on the line, as the owner's picture draws it; the group's name is its tooltip.
fn side_label(id: &str, name: &str) -> impl IntoElement {
    h_flex().w_full().justify_center().child(
        div()
            .id(SharedString::from(format!("side-by-side-{id}")))
            .debug_selector(|| "side-by-side".into())
            .flex_none()
            .py_0p5()
            .tooltip(Tooltip::text(name.to_owned()))
            .child(
                Label::new(SIDE_BY_SIDE)
                    .size(LabelSize::XSmall)
                    .weight(FontWeight::SEMIBOLD)
                    .color(Color::Muted),
            ),
    )
}

pub(super) const SIDE_BY_SIDE: &str = "SIDE BY SIDE";
