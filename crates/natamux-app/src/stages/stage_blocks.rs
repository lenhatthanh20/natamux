//! The blocks of the stage list: a side by side group, and a branch with a block for each path.
//! The rows inside are in `stage_rows`.

use gpui::{ClickEvent, FontWeight, KeyDownEvent};
use natamux_proto::stages::pipeline_rules::{IssueLevel, PipelineIssue};
use natamux_proto::stages::position::describe_condition;
use natamux_proto::stages::schema::{BranchPath, StageBranch, StageGroup, Step};
use ui::prelude::*;

use super::editor_labels::{branch_name, group_name, path_name};
use super::kind_icon::branch_icon;
use super::path_add::open_path_add;
use super::stage_list::{StageList, issue_lines};
use super::stage_rows::{Ctx, ROW_HEIGHT, caps, grip, more_button, sized};
use crate::components::tab_order;

impl StageList {
    pub(super) fn group_block(
        &mut self,
        group: &StageGroup,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group.id.clone();
        let mine = ctx.of(&id);
        let hover_group = SharedString::from(format!("group-row-{id}"));
        let (name, menu_id) = (group_name(group), id.clone());
        let head = h_flex()
            .id(SharedString::from(format!("group-head-{id}")))
            .group(hover_group.clone())
            .h(rems_from_px(22.0_f32))
            .gap_1p5()
            .px_1p5()
            .cursor_grab()
            .child(grip(11., cx))
            .child(div().flex_1().child(caps(&name)))
            .child(more_button(
                &id,
                hover_group,
                false,
                &name,
                cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.open_group_menu(&menu_id, event.position(), window, cx);
                }),
            ));
        let head = Self::drag_source(head, &id, name);
        let rows: Vec<AnyElement> = group
            .parallel
            .iter()
            .map(|stage| self.stage_row(stage, ctx, window, cx))
            .collect();
        let border = cx.theme().colors().border;
        let block = v_flex()
            .id(SharedString::from(format!("group-{id}")))
            .relative()
            .my(px(3.))
            .pb(px(2.))
            .rounded(px(6.))
            .border_1()
            .border_color(border)
            .child(head)
            .when(!mine.is_empty(), |this| {
                this.child(div().px_1().pb_1().child(issue_lines(&mine, cx)))
            })
            .child(v_flex().children(rows))
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }

    pub(super) fn branch_block(
        &mut self,
        branch: &StageBranch,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = branch.id.clone();
        let mine = ctx.of(&id);
        let head = self.branch_head(branch, ctx.selected == Some(id.as_str()), &mine, cx);
        let paths: Vec<AnyElement> = branch
            .paths
            .iter()
            .map(|path| self.path_block(path, ctx, window, cx))
            .collect();
        let accent = cx.theme().colors().text_accent;
        let block = v_flex()
            .id(SharedString::from(format!("branch-{id}")))
            .relative()
            .my(px(3.))
            .pl(px(4.))
            .border_l_2()
            .border_color(accent)
            .child(head)
            .children(paths)
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }

    /// One path of a branch: its name and condition, then its stages and groups.
    fn path_block(
        &mut self,
        path: &BranchPath,
        ctx: &Ctx,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stages: Vec<AnyElement> = path
            .stages
            .iter()
            .filter_map(|item| match item {
                Step::Stage(stage) => Some(self.stage_row(stage, ctx, window, cx)),
                Step::Group(group) => Some(self.group_block(group, ctx, window, cx)),
                // The rules refuse a branch in a path, and the draft never reads one.
                Step::Branch(_) => None,
            })
            .collect();
        let condition = if path.when.is_none() {
            "Otherwise".to_owned()
        } else {
            format!("if {}", describe_condition(path))
        };
        let title = h_flex()
            .id(SharedString::from(format!("path-{}", path.id)))
            .h(rems_from_px(20.0_f32))
            .gap_1p5()
            .overflow_hidden()
            .child(caps(&path_name(path)))
            .child(sized(condition, 11.).color(Color::Muted).truncate());
        let title = self.drop_zone(title, &path.id, cx);
        let body = if stages.is_empty() {
            self.empty_path(&path.id, cx).into_any_element()
        } else {
            v_flex().children(stages).into_any_element()
        };
        v_flex()
            .py(px(2.))
            .pl(px(14.))
            .child(title)
            .child(body)
            .into_any_element()
    }

    /// "No stage yet" and the Add link of a path without stages.
    fn empty_path(&self, path_id: &str, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let path_id = path_id.to_owned();
        h_flex()
            .h(rems_from_px(24.0_f32))
            .pl(rems_from_px(24.0_f32))
            .gap_2()
            .child(sized("No stage yet", 12.).color(Color::Placeholder))
            .child(
                div()
                    .id(SharedString::from(format!("add-{path_id}")))
                    .cursor_pointer()
                    .child(sized("Add", 12.).color(Color::Accent))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        open_path_add(&this.editor, &path_id, event.position(), window, cx);
                    })),
            )
    }

    /// The header of a branch block: its row (pick, menu) and its problems.
    fn branch_head(
        &mut self,
        branch: &StageBranch,
        picked: bool,
        mine: &[PipelineIssue],
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors();
        let (hover, selected_bg) = (colors.ghost_element_hover, colors.ghost_element_selected);
        let id = branch.id.clone();
        let name = branch_name(branch);
        let handle = self.focus_of(&id, cx);
        tab_order::push(&handle);
        let failing = mine.iter().any(|issue| issue.level == IssueLevel::Error);
        let hover_group = SharedString::from(format!("row-{id}"));
        let (key_id, menu_id) = (id.clone(), id.clone());
        let button = h_flex()
            .id(SharedString::from(format!("stage-row-{id}")))
            .track_focus(&handle)
            .flex_1()
            .min_w_0()
            .h_full()
            .gap_1p5()
            .rounded_sm()
            .cursor_pointer()
            .on_click(self.editor_click(id.clone()))
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                    this.select(&key_id, cx);
                }
            }))
            .child(branch_icon().size(IconSize::Small).color(Color::Accent))
            .child(
                div().flex_1().min_w_0().child(
                    sized(name.clone(), 13.)
                        .weight(FontWeight::MEDIUM)
                        .color(if failing {
                            Color::Error
                        } else {
                            Color::Default
                        })
                        .truncate(),
                ),
            )
            .child(sized("If", 11.).color(Color::Accent));
        let line = h_flex()
            .id(SharedString::from(format!("row-{id}")))
            .group(hover_group.clone())
            .h(px(ROW_HEIGHT))
            .flex_none()
            .gap_1p5()
            .pl(px(2.))
            .pr(px(4.))
            .rounded(px(5.))
            .when(picked, |this| this.bg(selected_bg))
            .when(!picked, |this| this.hover(|style| style.bg(hover)))
            .child(grip(12., cx))
            .child(button)
            .child(more_button(
                &id,
                hover_group,
                picked,
                &name,
                cx.listener(move |this, event: &ClickEvent, window, cx| {
                    this.open_branch_menu(&menu_id, event.position(), window, cx);
                }),
            ));
        let line = Self::drag_source(line, &id, name);
        v_flex()
            .child(line)
            .when(!mine.is_empty(), |this| {
                this.child(div().px_1().pb_1().child(issue_lines(mine, cx)))
            })
            .into_any_element()
    }
}
