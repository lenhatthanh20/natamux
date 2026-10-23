//! The pipeline preview of the Stages tab (`PipelinePreview.tsx`, docs/ui-spec.md 4.32): the
//! pipeline as the draft has it, one node per step, a box around stages that run side by side, and
//! a fork for a branch. A click on a node picks its stage. Still: nothing moves.

use std::collections::HashSet;

use gpui::{AnyElement, Div, Entity, FontWeight, Stateful};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::resolve::active_steps;
use natamux_proto::stages::schema::{Stage, Step};
use ui::{Tooltip, prelude::*};

use super::editor_labels::{StageRunner, stage_runner};
use super::kind_icon::{kind_icon, prompt_icon, waits_icon};
use super::pipeline_editor::PipelineEditor;
use super::pipeline_preview_branch::{branch_chart, branch_notes};
use super::preview_rules::{
    NO_STAGE_ON, agent_line_text, error_ids, group_list_label, tip_of, waits_for_person,
};
use crate::agents::agent_choices::agent_label;
use crate::components::agent_badge::AgentIcon;
use crate::stages::editor_fields::has_custom_prompt;

/// A node is 104 wide; its ring is 24.
const NODE_WIDTH: f32 = 104.;
const RING: f32 = 24.;
const NODE_TOP: f32 = 8.;
/// Where the line meets the first node: its top padding plus half the ring, less half the line.
const LINE_Y: f32 = NODE_TOP + RING / 2. - 1.;

/// What every part of the chart reads.
pub(super) struct Chart {
    pub steps: Vec<Step>,
    pub selected: Option<String>,
    pub wrong: HashSet<String>,
    pub editor: Entity<PipelineEditor>,
}

/// The dashed line between two nodes.
pub(super) fn link(offset: f32, cx: &App) -> impl IntoElement + use<> {
    div()
        .flex_none()
        .w(rems_from_px(16.0_f32))
        .h(rems_from_px(0.0_f32))
        .mt(px(offset))
        .border_t_2()
        .border_dashed()
        .border_color(cx.theme().colors().border)
}

pub(super) fn select_on_click(
    editor: &Entity<PipelineEditor>,
    id: &str,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static + use<> {
    let (editor, id) = (editor.clone(), id.to_owned());
    move |_, _, cx| editor.update(cx, |editor, cx| editor.select(&id, cx))
}

/// A node's box: hover and selected background, 104 wide.
pub(super) fn node_box(id: SharedString, selected: bool, cx: &App) -> Stateful<Div> {
    let colors = cx.theme().colors();
    v_flex()
        .id(id)
        .flex_none()
        .w(px(NODE_WIDTH))
        .items_center()
        .gap_1()
        .px_1()
        .pt(px(NODE_TOP))
        .pb_2()
        .rounded_md()
        .cursor_pointer()
        .when(selected, |this| this.bg(colors.element_selected))
        .when(!selected, |this| {
            this.hover(|style| style.bg(colors.element_hover))
        })
}

fn ring(stage: &Stage, error: bool, cx: &App) -> impl IntoElement + use<> {
    let colors = cx.theme().colors();
    let (border, icon_color) = if error {
        (cx.theme().status().error, Color::Error)
    } else {
        (colors.border, Color::Muted)
    };
    let mark = |icon: Icon, top: f32| {
        div()
            .absolute()
            .top(px(top))
            .right(px(-7.))
            .size(rems_from_px(14.0_f32))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .child(icon.size(IconSize::Indicator).color(Color::Muted))
    };
    div()
        .relative()
        .flex_none()
        .size(px(RING))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .border_1()
        .border_dashed()
        .border_color(border)
        .child(
            kind_icon(stage.kind)
                .size(IconSize::XSmall)
                .color(icon_color),
        )
        .when(waits_for_person(stage), |this| {
            this.child(mark(waits_icon(), -5.))
        })
        .when(has_custom_prompt(stage), |this| {
            this.child(mark(prompt_icon(), 15.))
        })
}

fn agent_line(runner: &StageRunner) -> impl IntoElement + use<> {
    h_flex()
        .max_w_full()
        .min_w_0()
        .gap_1()
        .child(AgentIcon::new(runner.agent.clone()))
        .child(
            Label::new(agent_line_text(runner, |agent| agent_label(Some(agent))))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
        )
}

/// One stage as a node: the ring with its marks, the name, and the agent line.
pub(super) fn stage_node(chart: &Chart, stage: &Stage, cx: &App) -> AnyElement {
    let runner = stage_runner(&chart.steps, stage);
    let selected = chart.selected.as_deref() == Some(stage.id.as_str());
    let error = chart.wrong.contains(&stage.id);
    let tip = tip_of(stage, runner.as_ref(), |agent| agent_label(Some(agent)));
    node_box(
        SharedString::from(format!("preview-{}", stage.id)),
        selected,
        cx,
    )
    .child(ring(stage, error, cx))
    .child(
        Label::new(stage_name(stage))
            .size(LabelSize::Small)
            .color(if error { Color::Error } else { Color::Default })
            .truncate(),
    )
    .children(runner.as_ref().map(agent_line))
    .tooltip(Tooltip::text(tip))
    .on_click(select_on_click(&chart.editor, &stage.id))
    .into_any_element()
}

/// Stages that run side by side: a dashed box with one node under the other.
fn group_box(
    chart: &Chart,
    group: &natamux_proto::stages::schema::StageGroup,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let error = chart.wrong.contains(&group.id);
    v_flex()
        .id(SharedString::from(format!("preview-group-{}", group.id)))
        .flex_none()
        .p(rems_from_px(3.0_f32))
        // The first node's ring sits on the main line.
        .mt(rems_from_px(-5.0_f32))
        .rounded_md()
        .border_1()
        .border_dashed()
        .border_color(if error {
            cx.theme().status().error
        } else {
            colors.border
        })
        .tooltip(Tooltip::text(group_list_label(group)))
        .children(
            group
                .parallel
                .iter()
                .map(|stage| stage_node(chart, stage, cx)),
        )
        .into_any_element()
}

/// The preview box. It scrolls sideways when the chart is wide.
pub fn pipeline_preview(editor: &Entity<PipelineEditor>, cx: &mut App) -> AnyElement {
    let colors = cx.theme().colors();
    let (box_bg, box_border) = (colors.elevated_surface_background, colors.border);
    let chart = {
        let state = editor.read(cx);
        Chart {
            steps: state.steps().to_vec(),
            selected: state.selected_id(),
            wrong: error_ids(state.issues()),
            editor: editor.clone(),
        }
    };
    let shown = active_steps(&chart.steps);
    let frame = v_flex()
        .id("pipeline-preview")
        .flex_none()
        .rounded_md()
        .border_1()
        .border_color(box_border)
        .bg(box_bg);
    if shown.is_empty() {
        return frame
            .child(
                div()
                    .px_4()
                    .py_5()
                    .child(Label::new(NO_STAGE_ON).color(Color::Muted)),
            )
            .into_any_element();
    }
    let branches: Vec<_> = shown
        .iter()
        .enumerate()
        .filter_map(|(index, step)| match step {
            Step::Branch(branch) => Some((branch, shown.get(index + 1))),
            _ => None,
        })
        .collect();
    let mut row = h_flex().items_start();
    for (index, step) in shown.iter().enumerate() {
        if index > 0 {
            row = row.child(link(LINE_Y, cx));
        }
        row = match step {
            Step::Stage(stage) => row.child(stage_node(&chart, stage, cx)),
            Step::Group(group) => row.child(group_box(&chart, group, cx)),
            Step::Branch(branch) => {
                row.child(branch_chart(&chart, branch, shown.get(index + 1), cx))
            }
        };
    }
    let top = if branches.is_empty() { 12. } else { 40. };
    frame
        .child(
            div()
                .id("pipeline-scroll")
                .overflow_x_scroll()
                .px_4()
                .pt(px(top))
                .pb_3()
                .child(row),
        )
        .children(
            branches
                .into_iter()
                .map(|(branch, next)| branch_notes(branch, next, cx)),
        )
        .into_any_element()
}

pub(super) fn bold(text: impl Into<SharedString>) -> Label {
    Label::new(text)
        .size(LabelSize::Small)
        .weight(FontWeight::SEMIBOLD)
}
