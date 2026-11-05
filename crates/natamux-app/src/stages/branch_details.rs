//! The branch block of the stage card list (layout-b 5.2.3): the "If" row, which opens the branch
//! sheet, then each path behind a guide line: its caption with the condition, and its stages
//! and groups, or "No stage yet" with Add.

use gpui::ClickEvent;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::{BranchPath, StageBranch, Step};
use ui::prelude::*;

use super::editor_labels::{branch_name, path_name};
use super::kind_icon::branch_icon;
use super::list_rules::{branch_meta, path_condition};
use super::path_add::open_path_add;
use super::stage_list::StageList;
use super::stage_rows::{caption, grip, guide, issues_of, more_button, own_clicks, problem_mark};
use crate::components::button_style::{button_icon, quiet_button};
use crate::components::chips::label_chip;
use crate::components::colors::TileColor;
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::components::text_size::row_text;

impl StageList {
    pub(super) fn branch_block(
        &mut self,
        branch: &StageBranch,
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = branch.id.clone();
        let head = self.branch_row(branch, issues, window, cx);
        let paths: Vec<AnyElement> = branch
            .paths
            .iter()
            .enumerate()
            .map(|(index, path)| self.path_block(index, path, issues, window, cx))
            .collect();
        let block = v_flex()
            .id(SharedString::from(format!("branch-{id}")))
            .relative()
            .w_full()
            .child(head)
            .children(paths)
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }

    fn branch_row(
        &mut self,
        branch: &StageBranch,
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = branch.id.clone();
        let name = branch_name(branch);
        let mine = issues_of(issues, &id);
        let menu_id = id.clone();
        let more = more_button(
            &id,
            &name,
            cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_branch_menu(&menu_id, event.position(), window, cx);
            }),
        );
        let tile =
            SettingsTile::icon(branch_icon(), TileColor::Violet, TileSize::Row).glyph_px(12.);
        let line = self
            .row_frame(&id, window, cx)
            .child(grip(&id, cx))
            .child(tile)
            .child(
                div()
                    .min_w_0()
                    .child(Label::new(name.clone()).size(row_text()).truncate()),
            )
            .child(label_chip("If"))
            .children(problem_mark(&id, &mine, cx))
            .child(div().flex_1())
            .child(
                Label::new(branch_meta(branch))
                    .size(LabelSize::Small)
                    .color(Color::Muted)
                    .single_line(),
            )
            .child(own_clicks(more));
        Self::drag_source(line, &id, name).into_any_element()
    }

    /// One path: its caption, then its stages and groups.
    fn path_block(
        &mut self,
        index: usize,
        path: &BranchPath,
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows: Vec<AnyElement> = path
            .stages
            .iter()
            .filter_map(|item| match item {
                Step::Stage(stage) => Some(self.stage_row(stage, issues, window, cx)),
                Step::Group(group) => Some(self.group_block(group, issues, window, cx)),
                // The rules refuse a branch in a path, and the draft never reads one.
                Step::Branch(_) => None,
            })
            .collect();
        let title = format!("{}. {}", index + 1, path_name(path));
        let head = caption(&title, Some(path_condition(path)))
            .id(SharedString::from(format!("path-{}", path.id)))
            .pl(rems_from_px(16.0_f32))
            .pr(rems_from_px(12.0_f32));
        let head = self.drop_zone(head, &path.id, cx);
        let body = if rows.is_empty() {
            self.empty_path(&path.id).into_any_element()
        } else {
            v_flex().children(rows).into_any_element()
        };
        guide(cx).child(head).child(body).into_any_element()
    }

    /// "No stage yet" and the Add button of a path without stages.
    fn empty_path(&self, path_id: &str) -> impl IntoElement + use<> {
        let (pipeline, path_id) = (self.editor.clone(), path_id.to_owned());
        let add = quiet_button(SharedString::from(format!("add-{path_id}")), "Add")
            .size(ButtonSize::Compact)
            .label_size(LabelSize::Small)
            .start_icon(button_icon("plus").size(IconSize::XSmall))
            .on_click(move |event: &ClickEvent, window, cx| {
                open_path_add(&pipeline, &path_id, event.position(), window, cx);
            });
        h_flex()
            .h(rems_from_px(32.0_f32))
            .pl(rems_from_px(16.0_f32))
            .gap(rems_from_px(8.0_f32))
            .child(
                Label::new("No stage yet")
                    .size(LabelSize::Small)
                    .color(Color::Placeholder),
            )
            .child(add)
    }
}
