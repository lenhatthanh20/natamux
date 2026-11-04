//! The details of a branch (`BranchDetails.tsx`): a short summary of its paths with Edit
//! branch, which opens the branch dialog, and the stages of each path with Add a stage.

use gpui::{AnyElement, ClickEvent, Entity, FontWeight, rems};
use natamux_proto::stages::capabilities::stage_name;
use natamux_proto::stages::catalog::catalog_entry;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{BranchPath, StageBranch, StageKind, Step};
use ui::prelude::*;

use super::branch_dialog;
use super::editor_catalog::{add_catalog_stage, catalog_choices};
use super::editor_labels::{branch_name, path_label, path_name};
use super::kind_icon::{branch_icon, kind_icon};
use super::pipeline_editor::PipelineEditor;
use super::stage_details::DetailsPane;
use super::stage_fields_view::{
    Choice, change, field_label, help_line, issue_list, open_choices, select,
};
use crate::components::button_style::quiet_button;

impl DetailsPane {
    pub(super) fn render_branch(
        &self,
        branch: &StageBranch,
        steps: &[Step],
        issues: &[PipelineIssue],
        cx: &App,
    ) -> AnyElement {
        let name = branch_name(branch);
        let pipeline = &self.editor;
        let summary = branch.paths.iter().enumerate().map(|(index, path)| {
            Label::new(format!("{}. {}", index + 1, path_label(path)))
                .size(LabelSize::Small)
                .color(Color::Muted)
        });
        let (edit_pipeline, branch_id) = (pipeline.clone(), branch.id.clone());
        let edit = quiet_button("edit-branch", "Edit branch\u{2026}")
            .start_icon(Icon::new(IconName::Pencil).size(IconSize::Small))
            .on_click(move |_, window, cx| {
                branch_dialog::open_edit(&edit_pipeline, &branch_id, window, cx)
            });
        let skip = branch.paths.last().is_some_and(|path| path.when.is_some());
        let paths = branch.paths.iter().enumerate().map(|(index, path)| {
            let label = format!("Path {}", index + 1);
            let title = format!("{label} \u{b7} {}", path_name(path));
            v_flex()
                .gap_1p5()
                .child(Label::new(title).weight(FontWeight::SEMIBOLD))
                .child(stages_of_path(path, &label, steps, pipeline, cx))
        });
        v_flex()
            .gap_4()
            .child(branch_header(branch, &name, cx))
            .children(issue_list(issues, cx))
            .child(
                v_flex()
                    .gap_1()
                    .children(summary)
                    .when(skip, |this| {
                        this.child(help_line(
                            "No path holds: the task skips this branch and goes on.",
                        ))
                    })
                    .child(h_flex().pt_1().child(edit)),
            )
            .child(v_flex().gap_3().children(paths))
            .into_any_element()
    }
}

fn branch_header(branch: &StageBranch, name: &str, cx: &App) -> impl IntoElement {
    v_flex()
        .gap_0p5()
        .child(
            h_flex()
                .gap_2()
                .child(branch_icon().size(IconSize::Medium).color(Color::Accent))
                .child(
                    Label::new(name.to_owned())
                        .size(LabelSize::Custom(rems(15. / 16.)))
                        .weight(FontWeight::SEMIBOLD)
                        .truncate(),
                ),
        )
        .child(
            h_flex()
                .flex_wrap()
                .gap_1()
                .child(
                    Label::new("Branch \u{b7}")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .child(
                    Label::new(branch.id.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .child(
                    Label::new(
                        "\u{b7} A task takes the first path whose condition holds, and runs only \
                         its stages.",
                    )
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
        )
}

fn stages_of_path(
    path: &BranchPath,
    label: &str,
    steps: &[Step],
    pipeline: &Entity<PipelineEditor>,
    cx: &App,
) -> impl IntoElement {
    let hover = cx.theme().colors().ghost_element_hover;
    // A group's stages are listed one by one, each marked as running side by side.
    let stages = path.stages.iter().flat_map(|item| {
        let lane = item.is_group();
        item.stages().into_iter().map(move |stage| (stage, lane))
    });
    let rows = stages.map(|(stage, lane)| {
        let (pipeline, id) = (pipeline.clone(), stage.id.clone());
        h_flex()
            .id(SharedString::from(format!("path-stage-{}", stage.id)))
            .gap_2()
            .px_1p5()
            .py_1()
            .rounded_md()
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .on_click(move |_, _, cx| pipeline.update(cx, |pipeline, cx| pipeline.select(&id, cx)))
            .child(
                kind_icon(stage.kind)
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(Label::new(stage_name(stage)).truncate())
            .when(lane, |this| {
                this.child(
                    Label::new("side by side")
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
            })
    });
    let (add_pipeline, path_id) = (pipeline.clone(), path.id.clone());
    let choices: Vec<String> = catalog_choices(steps)
        .into_iter()
        .filter(|entry| !matches!(entry.stage.kind, StageKind::Context | StageKind::Release))
        .map(|entry| entry.stage.id.clone())
        .collect();
    let add = select(
        SharedString::from(format!("add-stage-{path_id}")),
        "Add a stage\u{2026}",
    )
    .on_click(move |event: &ClickEvent, window, cx| {
        let choices = choices
            .iter()
            .filter_map(|id| catalog_entry(id))
            .map(|entry| {
                let (pipeline, path_id, id) = (
                    add_pipeline.clone(),
                    path_id.clone(),
                    entry.stage.id.clone(),
                );
                Choice::new(stage_name(&entry.stage), None, move |_, cx| {
                    change(&pipeline, cx, |steps| {
                        add_catalog_stage(steps, &id, Some(&path_id))
                    })
                })
            })
            .collect();
        open_choices(choices, event.position(), window, cx);
    })
    .tooltip(ui::Tooltip::text(format!("Add a stage to {label}")));
    v_flex()
        .gap_1p5()
        .child(field_label("Stages, one after another"))
        .when(path.stages.is_empty(), |this| {
            this.child(help_line("No stage yet. Add one below."))
        })
        .child(v_flex().gap_0p5().children(rows))
        .child(add)
}
