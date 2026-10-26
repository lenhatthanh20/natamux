//! The header of the details panel (T-0171, T-0183): the view's name, the task id and stage on
//! Progress, then "..." and Close or Back. Beside an open task the views and "..." are on the
//! rail (`details_rail.rs`); over the list the header keeps them.

use natamux_proto::tasks::task::Task;
use ui::{Tab, Tooltip, prelude::*};

use super::DetailsPanel;
use crate::app::layout_mode::DetailsMode;
use crate::app::tab_rules::details_tabs;
use crate::app::view_rules::DetailsView;
use crate::components::chips::tip_with_keys;
use crate::components::text_size::row_text;
use crate::tasks::task_rules::stage_label;

impl DetailsPanel {
    /// The actions at the right end of the header: "..." (on the rail beside an open task),
    /// then Close or Back. A dock has no close button, as Zed's: the status bar toggle closes it.
    /// Edit is in the "..." menu.
    pub(super) fn header_actions(
        &self,
        task: Option<&Task>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let task = task.filter(|_| !self.rail_shown(cx));
        let close = match self.mode {
            DetailsMode::Docked => None,
            DetailsMode::Page => Some(
                IconButton::new("details-back", IconName::ArrowLeft)
                    .tooltip(Tooltip::text("Back"))
                    .on_click(cx.listener(|panel, _, _, cx| panel.close(cx))),
            ),
            DetailsMode::Sheet => Some(
                IconButton::new("details-close", IconName::Close)
                    .tooltip(tip_with_keys("Close details", "Escape"))
                    .on_click(cx.listener(|panel, _, _, cx| panel.close(cx))),
            ),
        };
        h_flex()
            .flex_none()
            .gap_1()
            .children(task.map(|task| self.more_button(task, cx)))
            .children(close)
    }

    /// The 32 px bar of the dock, as Zed's `TabBar` without its tab cells and their borders.
    pub(super) fn header_bar(cx: &App) -> gpui::Div {
        h_flex()
            .flex_none()
            .w_full()
            .h(Tab::container_height(cx))
            .gap_1()
            .px_1()
            .bg(cx.theme().colors().tab_bar_background)
            .border_b_1()
            .border_color(cx.theme().colors().border)
    }

    /// The dock header (T-0171, T-0183): the view's name, and on Progress the task id and stage;
    /// then the actions at the right. Beside an open task the views are on the rail; over the
    /// list there is no rail, so the header keeps them as icon buttons.
    pub(super) fn render_tabs(
        &self,
        task: &Task,
        view: DetailsView,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title = task.title.clone();
        let task_line = h_flex()
            .id("details-task")
            .flex_1()
            .min_w_0()
            .gap_1p5()
            .pl_1()
            .tooltip(Tooltip::text(title))
            .child(Label::new(view_name(view)).size(row_text()))
            .when(view == DetailsView::Progress, |line| {
                line.child(
                    Label::new(task.id.clone())
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                        .buffer_font(cx),
                )
                .children(stage_label(task).map(|stage| {
                    div().min_w_0().child(
                        Label::new(stage)
                            .size(row_text())
                            .color(Color::Muted)
                            .truncate(),
                    )
                }))
            });
        let views = (!self.rail_shown(cx)).then(|| self.view_buttons(task, view, cx));
        Self::header_bar(cx)
            .child(task_line)
            .children(views)
            .child(self.header_actions(Some(task), cx))
    }

    /// The views as icon buttons, for the header while there is no rail (over the list).
    fn view_buttons(&self, task: &Task, view: DetailsView, cx: &mut Context<Self>) -> gpui::Div {
        let views = details_tabs(task.workspace.is_some(), false);
        h_flex()
            .flex_none()
            .gap_0p5()
            .children(views.into_iter().map(|this| {
                let (id, icon, label) = match this {
                    DetailsView::Files => ("details-tab-files", IconName::Folder, "Files"),
                    DetailsView::Details => ("details-tab-details", IconName::Info, "Details"),
                    _ => ("details-tab-progress", IconName::ListTodo, "Progress"),
                };
                let picked = view == this;
                IconButton::new(id, icon)
                    .icon_size(IconSize::Small)
                    .icon_color(if picked { Color::Default } else { Color::Muted })
                    .selected_icon_color(Color::Default)
                    .toggle_state(picked)
                    .tooltip(Tooltip::text(label))
                    .on_click(cx.listener(move |panel, _, _, cx| {
                        panel
                            .dashboard
                            .update(cx, |dashboard, cx| dashboard.set_details_view(this, cx));
                    }))
            }))
    }

    /// The rail shows beside an open task (T-0183): it has the views and "...".
    fn rail_shown(&self, cx: &App) -> bool {
        self.dashboard.read(cx).open_task.is_some()
    }

    /// The rail's "...": the menu of the task the panel shows.
    pub fn rail_more_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let task = self.task(cx)?;
        Some(self.more_button(&task, cx).into_any_element())
    }
}

/// The name of a view, in the panel's header.
fn view_name(view: DetailsView) -> &'static str {
    match view {
        DetailsView::Progress => "Progress",
        DetailsView::Changes => "Changes",
        DetailsView::History => "History",
        DetailsView::Docs => "Docs",
        DetailsView::Files => "Files",
        DetailsView::Details => "Details",
    }
}
