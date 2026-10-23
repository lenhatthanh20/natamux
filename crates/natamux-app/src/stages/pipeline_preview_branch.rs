//! A branch in the pipeline preview: the diamond "If" node, a dashed fork with one lane for each
//! path (its condition above the stages), the "No path holds" line, and the words under the chart
//! that say each path (`BranchChart` and `BranchNotes` of `PipelinePreview.tsx`).

use gpui::{AnyElement, FontWeight};
use natamux_proto::stages::schema::{BranchPath, Condition, StageBranch, Step};
use ui::{Tooltip, prelude::*};

use super::editor_labels::{branch_label, branch_name, path_name};
use super::pipeline_preview::{Chart, bold, link, node_box, select_on_click, stage_node};
use super::preview_rules::{branch_note_head, fork_label, path_note, skip_text, when_tip};
use crate::components::icons::icon_path;

/// The diamond node where the paths split.
fn if_node(chart: &Chart, branch: &StageBranch, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let selected = chart.selected.as_deref() == Some(branch.id.as_str());
    let error = chart.wrong.contains(&branch.id);
    let diamond_color = if error {
        cx.theme().status().error
    } else {
        colors.text_accent
    };
    node_box(
        SharedString::from(format!("preview-{}", branch.id)),
        selected,
        cx,
    )
    .child(
        div()
            .flex_none()
            .size(rems_from_px(24.0_f32))
            .flex()
            .items_center()
            .justify_center()
            .child(
                gpui::svg()
                    .path(icon_path("diamond"))
                    .size(rems_from_px(20.0_f32))
                    .text_color(diamond_color),
            ),
    )
    .child(
        Label::new("If")
            .size(LabelSize::Small)
            .weight(FontWeight::SEMIBOLD)
            .color(Color::Accent),
    )
    .child(
        Label::new(branch_name(branch))
            .size(LabelSize::Small)
            .color(if error { Color::Error } else { Color::Default })
            .truncate(),
    )
    .tooltip(Tooltip::text(branch_label(branch)))
    .on_click(select_on_click(&chart.editor, &branch.id))
    .into_any_element()
}

/// The If node, the line, and the fork.
pub(super) fn branch_chart(
    chart: &Chart,
    branch: &StageBranch,
    next: Option<&Step>,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let error = chart.wrong.contains(&branch.id);
    let node = if_node(chart, branch, cx);
    let skip = skip_text(branch, next);
    let lanes: Vec<AnyElement> = branch
        .paths
        .iter()
        .filter(|path| !path.stages.is_empty())
        .map(|path| lane(chart, branch, path, cx))
        .collect();
    h_flex()
        .id(SharedString::from(format!("preview-fork-{}", branch.id)))
        .flex_none()
        .items_start()
        .child(node)
        .child(link(18., cx))
        .child(
            v_flex()
                .id(SharedString::from(format!("preview-lanes-{}", branch.id)))
                .flex_none()
                // The first lane's line sits on the main line, its condition above it.
                .mt(rems_from_px(-32.0_f32))
                .p_1()
                .gap_1()
                .rounded_md()
                .border_1()
                .border_dashed()
                .border_color(if error {
                    cx.theme().status().error
                } else {
                    colors.border
                })
                .tooltip(Tooltip::text(fork_label(branch)))
                .children(lanes)
                .children(
                    skip.map(|text| Label::new(text).size(LabelSize::XSmall).color(Color::Muted)),
                ),
        )
        .into_any_element()
}

fn lane(chart: &Chart, branch: &StageBranch, path: &BranchPath, cx: &App) -> AnyElement {
    let mut stages = h_flex().items_start();
    for (index, stage) in path.stages.iter().filter(|stage| stage.is_on()).enumerate() {
        if index > 0 {
            stages = stages.child(link(18., cx));
        }
        stages = stages.child(stage_node(chart, stage, cx));
    }
    v_flex()
        .flex_none()
        .items_start()
        .child(when_chip(branch, path, cx))
        .child(stages)
        .into_any_element()
}

/// The condition of a lane: its name, then "if <file> has "<text>"", or Otherwise.
fn when_chip(branch: &StageBranch, path: &BranchPath, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let word = |text: &'static str| Label::new(text).size(LabelSize::Small).color(Color::Muted);
    let code = |text: String| Label::new(text).size(LabelSize::Small).buffer_font(cx);
    let chip = h_flex()
        .id(SharedString::from(format!(
            "preview-when-{}-{}",
            branch.id, path.id
        )))
        .flex_none()
        .gap_1()
        .px_2()
        .py_0p5()
        .rounded_full()
        .border_1()
        .border_color(colors.border)
        .bg(colors.element_background)
        .child(bold(path_name(path)).color(Color::Accent))
        .tooltip(Tooltip::text(when_tip(path)));
    let chip = match &path.when {
        None => chip.child(word("Otherwise")),
        Some(when) => {
            let chip = chip.child(word("if"));
            let chip = if path.not == Some(true) {
                chip.child(word("NOT"))
            } else {
                chip
            };
            match when {
                Condition::Command { run } => chip.child(code(run.clone())).child(word("succeeds")),
                Condition::Output {
                    file,
                    section,
                    contains,
                } => {
                    let target = section.as_ref().map_or_else(
                        || file.clone(),
                        |section| format!("{file} \u{203a} {section}"),
                    );
                    chip.child(code(target))
                        .child(word("has"))
                        .child(Label::new(format!("\"{contains}\"")).size(LabelSize::Small))
                }
            }
        }
    };
    chip.into_any_element()
}

/// The branch in words under the chart: the paths in order, with their conditions and stages.
pub(super) fn branch_notes(branch: &StageBranch, next: Option<&Step>, cx: &App) -> AnyElement {
    let name = branch_name(branch);
    let head = branch_note_head(branch);
    let rest = head.strip_prefix(&name).unwrap_or(&head).to_owned();
    let paths = branch.paths.iter().map(|path| {
        let note = path_note(path);
        let own = path_name(path);
        let rest = note.strip_prefix(&own).unwrap_or(&note).to_owned();
        h_flex()
            .gap_0()
            .child(bold(own))
            .child(Label::new(rest).size(LabelSize::Small))
    });
    v_flex()
        .id(SharedString::from(format!("preview-notes-{}", branch.id)))
        .gap_1()
        .px_4()
        .py_3()
        .border_t_1()
        .border_color(cx.theme().colors().border)
        .child(
            h_flex()
                .child(bold(name))
                .child(Label::new(rest).size(LabelSize::Small)),
        )
        .child(v_flex().gap_0p5().pl_5().children(paths))
        .children(skip_text(branch, next).map(|text| {
            Label::new(format!("{text}."))
                .size(LabelSize::Small)
                .color(Color::Muted)
        }))
        .into_any_element()
}
