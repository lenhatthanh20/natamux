//! The branch of the Progress tab: a frame with the If header and one lane per path, stacked. A
//! lane shows its keyword, name, condition and tags, and its nodes; a path not taken folds to
//! that one row and opens on click. A lane renders its nodes with the column of steps, so a path
//! that holds a group or a branch (not in the schema yet) needs no new code.

use gpui::{AnyElement, FontWeight};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::schema::{BranchPath, StageBranch, Step};
use natamux_proto::tasks::task::TaskState;
use ui::{Tooltip, prelude::*};

use super::Ctx;
use super::flow::{Around, Edge, link_after};
use crate::components::chips::StateChip;
use crate::components::icons::icon_path;
use crate::stages::flow_lines::Port;
use crate::stages::flow_rules::{
    CaretAt, LaneState, branch_line, lane_keyword, lane_state, stage_count_label,
};
use crate::stages::stage_view_rules::{ProgressLookup, path_label};
use crate::tasks::details_commands::PanelCommand;

impl Ctx<'_> {
    /// The If frame with its lanes, and the connector to the next step under it.
    pub(super) fn branch(
        &self,
        branch: &StageBranch,
        own: Edge,
        around: Around,
        cx: &App,
    ) -> AnyElement {
        let lookup = |id: &str| self.by_id.get(id).map(|entry| entry.status);
        let lanes = branch.paths.iter().enumerate().map(|(index, path)| {
            let state = lane_state(branch, path, &lookup);
            self.lane(branch, index, path, state, cx)
        });
        let out = around.out_style(own);
        // The card of a selected node of a path sits right under the whole frame (mockup A).
        let details = self.details_of(
            branch.paths.iter().flat_map(|path| &path.stages),
            CaretAt::Middle,
            cx,
        );
        let open = details.is_some();
        let frame = v_flex()
            .relative()
            .w_full()
            .p_2()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().colors().border_variant)
            .child(branch_head(branch, &lookup, cx))
            .children(lanes)
            .children(around.into_style(own).map(Port::top))
            .children(out.filter(|_| !open).map(Port::bottom));
        v_flex()
            .w_full()
            .child(frame)
            .children(details)
            .children(out.map(|style| link_after(style, open)))
            .into_any_element()
    }

    /// One path: its head row, the condition, why it was taken, and its nodes. A path not taken
    /// is only the head row and the condition until it is opened.
    fn lane(
        &self,
        branch: &StageBranch,
        index: usize,
        path: &BranchPath,
        state: LaneState,
        cx: &App,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let key = format!("{}/{}", branch.id, path.id);
        let taken = state == LaneState::Taken;
        let folded = state == LaneState::NotTaken && !self.input.open_lanes.contains(&key);
        let keyword = lane_keyword(index, path);
        // `not` is the keyword "unless", so the condition reads positive after it.
        let positive = BranchPath {
            not: None,
            ..path.clone()
        };
        let when = path.when.is_some().then(|| describe_condition(&positive));
        let because = taken
            .then(|| {
                path.stages
                    .first()
                    .and_then(|stage| self.by_id.get(stage.id.as_str()))
                    .and_then(|entry| entry.because.clone())
            })
            .flatten();
        let head = self.lane_head(&key, keyword, path, state, folded, cx);
        let nodes = if folded {
            Vec::new()
        } else {
            let steps: Vec<Step> = path.stages.iter().cloned().map(Step::Stage).collect();
            self.column(&steps, true, cx)
        };
        v_flex()
            .w_full()
            .pl_2()
            .gap_1()
            .border_l_2()
            .border_color(if taken {
                cx.theme().status().success
            } else {
                colors.border_variant
            })
            .child(head)
            .children(when.map(|text| {
                let tip = format!("{keyword} {text}");
                div()
                    .id(SharedString::from(format!("when-{key}")))
                    .min_w_0()
                    .tooltip(Tooltip::text(tip))
                    .child(
                        Label::new(text)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .buffer_font(cx)
                            .truncate(),
                    )
            }))
            .children(because.map(|text| {
                Label::new(format!("because {text}"))
                    .size(LabelSize::XSmall)
                    .color(Color::Success)
            }))
            .when(!nodes.is_empty(), |this| {
                this.child(v_flex().w_full().pt_1().children(nodes))
            })
            .into_any_element()
    }

    /// The keyword, the name and the tags of a path; a path not taken is also the click target
    /// that folds and opens it.
    fn lane_head(
        &self,
        key: &str,
        keyword: &'static str,
        path: &BranchPath,
        state: LaneState,
        folded: bool,
        cx: &App,
    ) -> impl IntoElement {
        let foldable = state == LaneState::NotTaken;
        let muted = state != LaneState::Taken;
        let chip = match state {
            LaneState::Taken => Some(("Taken", TaskState::Done)),
            LaneState::NotTaken => Some(("Not taken", TaskState::Draft)),
            LaneState::NotReached => None,
        };
        let on = self.on.clone();
        let fold = PanelCommand::FoldLane(key.to_owned());
        let chevron = if folded {
            IconName::ChevronRight
        } else {
            IconName::ChevronDown
        };
        let hover = cx.theme().colors().ghost_element_hover;
        h_flex()
            .id(SharedString::from(format!("lane-{key}")))
            .w_full()
            .gap_1()
            .items_center()
            .rounded_sm()
            .when(foldable, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(move |_, window, cx| on(fold.clone(), window, cx))
                    .child(
                        Icon::new(chevron)
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                    )
            })
            .child(
                Label::new(keyword)
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .buffer_font(cx),
            )
            .child(
                div().min_w_0().child(
                    Label::new(path_label(path).to_owned())
                        .size(LabelSize::Small)
                        .weight(FontWeight::SEMIBOLD)
                        .color(if muted { Color::Muted } else { Color::Default })
                        .truncate(),
                ),
            )
            .child(div().flex_1().min_w_0())
            .child(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .children(chip.map(|(text, state)| StateChip::new(state).text(text)))
                    .child(
                        Label::new(stage_count_label(path.stages.len()))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
    }
}

/// The If header: the diamond tile, "If · <name>" and the one line of what the branch did.
fn branch_head(branch: &StageBranch, lookup: &ProgressLookup, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    let name = branch.name.clone().unwrap_or_else(|| {
        branch
            .paths
            .iter()
            .map(path_label)
            .collect::<Vec<_>>()
            .join(" or ")
    });
    let tile = div()
        .flex_none()
        .size(rems_from_px(24.0_f32))
        .flex()
        .items_center()
        .justify_center()
        .rounded_md()
        .border_1()
        .border_color(colors.border_variant)
        .bg(colors.element_background)
        .child(
            Icon::from_path(icon_path("diamond"))
                .size(IconSize::Small)
                .color(Color::Accent),
        );
    let title = h_flex()
        .min_w_0()
        .gap_1()
        .child(
            Label::new("If \u{b7}")
                .size(LabelSize::Small)
                .weight(FontWeight::SEMIBOLD)
                .color(Color::Accent),
        )
        .child(
            div().min_w_0().child(
                Label::new(name)
                    .size(LabelSize::Small)
                    .weight(FontWeight::SEMIBOLD)
                    .truncate(),
            ),
        );
    h_flex().w_full().gap_2().child(tile).child(
        v_flex().flex_1().min_w_0().child(title).child(
            Label::new(branch_line(branch, lookup))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        ),
    )
}
