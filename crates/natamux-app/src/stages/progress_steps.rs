//! The group row of the Progress tab: a pill with the group's name on the line, then its stages
//! side by side (two: fork and join curves; three or four: a dashed frame) and the join dot with
//! the note ("Review starts when both are finished"). The branch is in `progress_branch.rs`.

use gpui::AnyElement;
use natamux_proto::stages::progress::StageStatus;
use natamux_proto::stages::schema::{Stage, StageGroup, Step};
use ui::prelude::*;

use super::Ctx;
use super::flow::{Around, Edge, ported};
use crate::stages::flow_lines::{FlowLink, JoinDot, PairCurves, Port};
use crate::stages::flow_rules::{
    GroupLayout, LinkStyle, PAIR_GAP, group_layout, join_style, link_style,
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
                format!("{name} starts when both are finished")
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
            .child(pill(&name, cx))
            .child(body)
            .child(dot)
            .into_any_element()
    }

    /// Two nodes side by side, between the fork and the join curves.
    fn pair(&self, first: &Stage, second: &Stage, prev: StageStatus, cx: &App) -> AnyElement {
        let side = |stage: &Stage| {
            let edge = self.stage_edge(&stage.id);
            let into = link_style(prev, edge.status, edge.state);
            let card = self
                .card(&stage.id, true, cx)
                .unwrap_or_else(|| div().into_any_element());
            let node = ported(card, Some(into), Some(join_style(edge.status)))
                .flex_1()
                .min_w_0();
            (edge, into, node)
        };
        let (left, left_into, left_node) = side(first);
        let (right, right_into, right_node) = side(second);
        let join = PairCurves::join(join_style(left.status), join_style(right.status));
        v_flex()
            .w_full()
            .child(PairCurves::fork(left_into, right_into))
            .child(
                // Stretched: both nodes take the taller one's height, so the join curves start
                // at the same bottom ports.
                h_flex()
                    .w_full()
                    .items_stretch()
                    .gap(rems_from_px(PAIR_GAP))
                    .child(left_node)
                    .child(right_node),
            )
            .child(join)
            .into_any_element()
    }

    /// Three or four nodes, one under the other, in a dashed frame that carries the ports.
    fn stack(&self, group: &StageGroup, own: Edge, into: LinkStyle, cx: &App) -> AnyElement {
        let out = join_style(own.status);
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
                    .filter_map(|stage| self.card(&stage.id, false, cx)),
            )
            .child(Port::top(into))
            .child(Port::bottom(out));
        v_flex()
            .w_full()
            .child(FlowLink::new(into).at_top())
            .child(frame)
            .child(FlowLink::new(out))
            .into_any_element()
    }
}

/// The group's name on the line, as a pill.
fn pill(name: &str, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex().w_full().justify_center().child(
        div()
            .flex_none()
            .px_2()
            .py_0p5()
            .rounded_full()
            .border_1()
            .border_color(colors.border_variant)
            .bg(colors.panel_background)
            .child(
                Label::new(format!("{name} \u{b7} side by side"))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            ),
    )
}
