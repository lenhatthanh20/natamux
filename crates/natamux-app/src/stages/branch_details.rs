//! The details of a branch (`BranchDetails.tsx`): a header with Edit branch, which opens the
//! branch dialog, and its paths in order, each with its condition and its stages.

use gpui::{AnyElement, FontWeight};
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::schema::{BranchPath, StageBranch};
use ui::prelude::*;

use super::branch_dialog;
use super::editor_labels::{branch_name, path_name, step_name};
use super::kind_icon::branch_icon;
use super::stage_details::{DetailsPane, details_header, header_name, kind_box};
use super::stage_fields_view::{details_section, issue_list};
use crate::components::button_style::action_button;

const ABOUT: &str = "Branch \u{b7} A task takes the first path whose condition holds. \
                     If none holds, it skips this branch.";

impl DetailsPane {
    pub(super) fn render_branch(
        &self,
        branch: &StageBranch,
        issues: &[PipelineIssue],
        cx: &App,
    ) -> AnyElement {
        let (pipeline, branch_id) = (self.editor.clone(), branch.id.clone());
        let edit = action_button("edit-branch", "Edit branch")
            .start_icon(Icon::new(IconName::Pencil).size(IconSize::XSmall))
            .on_click(move |_, window, cx| {
                branch_dialog::open_edit(&pipeline, &branch_id, window, cx)
            });
        let icon = branch_icon().color(Color::Accent);
        let about = Label::new(ABOUT)
            .size(LabelSize::Small)
            .color(Color::Muted)
            .truncate();
        let title = v_flex()
            .flex_1()
            .min_w_0()
            .child(header_name(branch_name(branch)))
            .child(about);
        let header = details_header(cx)
            .child(kind_box(icon, false, cx))
            .child(title)
            .child(div().flex_none().child(edit));
        let paths = branch
            .paths
            .iter()
            .enumerate()
            .map(|(index, path)| path_row(index, path, cx));
        v_flex()
            .w_full()
            .child(header)
            .children(issue_list(issues, cx))
            .child(details_section(Some("Paths, in order")).children(paths))
            .into_any_element()
    }
}

/// "1. Name" and the condition, then the stages of the path, with a border under it.
fn path_row(index: usize, path: &BranchPath, cx: &App) -> impl IntoElement {
    let condition = if path.when.is_some() {
        format!("if {}", describe_condition(path))
    } else {
        "Otherwise".to_owned()
    };
    let names: Vec<String> = path.stages.iter().map(step_name).collect();
    let stages = if names.is_empty() {
        "No stage yet".to_owned()
    } else {
        names.join(" \u{2192} ")
    };
    let name = Label::new(format!("{}. {}", index + 1, path_name(path)))
        .size(LabelSize::Custom(rems_from_px(13.0_f32)))
        .weight(FontWeight::BOLD);
    let condition = Label::new(condition)
        .size(LabelSize::Custom(rems_from_px(13.0_f32)))
        .color(Color::Muted)
        .truncate();
    v_flex()
        .w_full()
        .py(rems_from_px(5.0_f32))
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .child(
            h_flex()
                .min_w_0()
                .gap_1()
                .child(div().flex_none().child(name))
                .child(div().min_w_0().child(condition)),
        )
        .child(
            Label::new(stages)
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
}
