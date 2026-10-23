//! The view header of All tasks: the title with the count, the state filter, Show finished (on
//! Board only) and the layout switch [Table | Board | Progress]. A child of `task_view.rs`, so it
//! reads the view's fields.

use gpui::FontWeight;
use ui::{Checkbox, prelude::*};

use super::TaskView;
use crate::app::view_state::Layout;
use crate::components::chips::CountBadge;
use crate::components::segmented::Segmented;
use crate::tasks::state_filter::{StateFilter, width_probe};
use crate::tasks::task_attention::{count_by_state, filter_tasks};
use crate::tasks::task_view_rules::{NARROW_BELOW, filter_states};

impl TaskView {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let main_width = *self.main_width.read(cx);
        let narrow = main_width.is_some_and(|width| width < NARROW_BELOW);
        let dashboard = self.dashboard.read(cx);
        let layout = dashboard.view().layout;
        let show_finished = dashboard.view().show_finished;
        let searched = filter_tasks(&dashboard.data.tasks, None, &dashboard.filter);
        let states = filter_states(&count_by_state(&searched), dashboard.state_filter);
        let filter = StateFilter::new(
            self.dashboard.clone(),
            states,
            dashboard.state_filter,
            main_width,
        );
        let count = self.shown_ids.len();
        let toggle = self.dashboard.clone();
        h_flex()
            .relative()
            .flex_none()
            .h(px(44.))
            .px_4()
            .gap_3()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(width_probe(&self.main_width))
            .child(
                Label::new("All tasks")
                    .size(LabelSize::Large)
                    .weight(FontWeight::SEMIBOLD),
            )
            .child(CountBadge::new(count))
            .child(filter)
            .child(
                h_flex()
                    .ml_auto()
                    .flex_none()
                    .gap_3()
                    .when(layout == Layout::Board, |row| {
                        row.child(
                            Checkbox::new("show-finished", show_finished.into())
                                .label(if narrow { "Finished" } else { "Show finished" })
                                .on_click(move |_, _, cx| {
                                    toggle.update(cx, |dashboard, cx| {
                                        dashboard.toggle_show_finished(cx)
                                    })
                                }),
                        )
                    })
                    .child(self.layout_switch(layout, narrow)),
            )
    }

    /// [Table | Board | Progress].
    fn layout_switch(&self, layout: Layout, narrow: bool) -> Segmented<3> {
        let pick = self.dashboard.clone();
        let layouts = [
            ("Table".into(), IconName::Table),
            ("Board".into(), IconName::DiffSplit),
            // lucide `list-checks` cannot go in a ToggleButtonWithIcon, which takes an IconName.
            ("Progress".into(), IconName::ListTodo),
        ];
        let selected = match layout {
            Layout::Table => 0,
            Layout::Board => 1,
            Layout::Progress => 2,
        };
        Segmented::new(
            "task-layout",
            &self.layout_focus,
            layouts,
            selected,
            move |index, _, cx| {
                let layout = match index {
                    0 => Layout::Table,
                    1 => Layout::Board,
                    _ => Layout::Progress,
                };
                pick.update(cx, |dashboard, cx| dashboard.set_layout(layout, cx));
            },
        )
        .icon_only(narrow)
    }
}
