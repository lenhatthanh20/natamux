//! The side by side group block of the stage card list (layout-b 5.2.2): a caption with its menu,
//! then the group's stage rows behind a guide line. The branch block is in `branch_details`.

use gpui::ClickEvent;
use natamux_proto::stages::pipeline_rules::PipelineIssue;
use natamux_proto::stages::schema::StageGroup;
use ui::prelude::*;

use super::editor_labels::group_name;
use super::list_rules::group_caption;
use super::stage_list::StageList;
use super::stage_rows::{caption, guide, issues_of, more_button, own_clicks, problem_mark};
use crate::components::button_style::button_icon;

impl StageList {
    pub(super) fn group_block(
        &mut self,
        group: &StageGroup,
        issues: &[PipelineIssue],
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = group.id.clone();
        let mine = issues_of(issues, &id);
        let menu_id = id.clone();
        let menu_name = group
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| "group".to_owned());
        let more = more_button(
            &id,
            &menu_name,
            cx.listener(move |this, event: &ClickEvent, window, cx| {
                this.open_group_menu(&menu_id, event.position(), window, cx);
            }),
        );
        let head = h_flex()
            .id(SharedString::from(format!("group-head-{id}")))
            .w_full()
            .px(rems_from_px(12.0_f32))
            .gap(rems_from_px(8.0_f32))
            .cursor_grab()
            .child(
                button_icon("columns-3")
                    .size(IconSize::XSmall)
                    .color(Color::Placeholder),
            )
            .child(
                caption(
                    &group_caption(group),
                    Some("\u{b7} start together".to_owned()),
                )
                .min_w_0(),
            )
            .children(problem_mark(&id, &mine, cx))
            .child(div().flex_1())
            .child(own_clicks(more));
        let head = Self::drag_source(head, &id, group_name(group));
        let rows: Vec<AnyElement> = group
            .parallel
            .iter()
            .map(|stage| self.stage_row(stage, issues, window, cx))
            .collect();
        let block = v_flex()
            .id(SharedString::from(format!("group-{id}")))
            .relative()
            .w_full()
            .child(head)
            .child(guide(cx).children(rows))
            .children(self.drop_line(&id, cx));
        self.drop_zone(block, &id, cx).into_any_element()
    }
}
