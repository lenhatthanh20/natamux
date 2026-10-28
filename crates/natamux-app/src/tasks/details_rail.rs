//! The rail of the details panel (T-0183): the panel's views as icon buttons in a column at the
//! right edge of the window, as Zed's and VS Code's side bars. It stays while the panel is
//! closed, so a view is one click away; a click on the picked view of an open panel closes it
//! (`rail_click`). Changes shows the count of files to commit. "..." at the bottom is the task's
//! menu, as in the panel's header over the list.

use gpui::{Entity, Subscription};
use ui::{FixedWidth, IconButtonShape, Tooltip, prelude::*};

use crate::app::commands::shortcut_of;
use crate::app::dashboard::Dashboard;
use crate::app::dashboard_data::Part;
use crate::app::tab_rules::details_tabs;
use crate::app::view_rules::DetailsView;
use crate::components::chips::tip_with_keys;
use crate::git::commit_rules::tab_count_label;
use crate::tasks::details_panel::DetailsPanel;

/// The rail's width; Root keeps it out of main.
pub const RAIL_WIDTH: f32 = 40.;
const RAIL_ICON: f32 = 18.;
/// A square of `ButtonSize::Large`'s 32 px: Zed's square shape sizes the button from a custom
/// icon size and outgrew the rail, so its hover spilled into the panel.
const RAIL_BUTTON: f32 = 32.;

/// A view's button: its id, icon, name and the command whose key the tooltip shows.
type ViewButton = (&'static str, IconName, &'static str, Option<&'static str>);

fn view_button(view: DetailsView) -> ViewButton {
    match view {
        DetailsView::Progress => (
            "rail-progress",
            IconName::ListTodo,
            "Progress",
            Some("progress.show"),
        ),
        DetailsView::Changes => (
            "rail-changes",
            IconName::FileDiff,
            "Changes",
            Some("tab.changes"),
        ),
        DetailsView::History => (
            "rail-history",
            IconName::GitGraph,
            "History",
            Some("tab.history"),
        ),
        DetailsView::Docs => (
            "rail-docs",
            IconName::FileTextOutlined,
            "Docs",
            Some("tab.docs"),
        ),
        DetailsView::Files => ("rail-files", IconName::Folder, "Files", Some("files.show")),
        DetailsView::Details => ("rail-details", IconName::Info, "Details", None),
    }
}

/// The Changes badge: the count of files to commit, only while there is one.
pub fn changes_badge(count: Option<usize>) -> Option<usize> {
    count.filter(|count| *count > 0)
}

/// A view's tooltip: Changes adds its count, as the tab did ("Changes, 5 files to commit").
pub fn view_tip(label: &str, badge: Option<usize>) -> String {
    match badge {
        Some(count) => format!("{label}, {}", tab_count_label(count as u64)),
        None => label.to_owned(),
    }
}

pub struct DetailsRail {
    dashboard: Entity<Dashboard>,
    /// The panel: its "..." menu sits at the bottom of the rail.
    details: Entity<DetailsPanel>,
    _observe: Subscription,
}

impl DetailsRail {
    pub fn new(
        dashboard: Entity<Dashboard>,
        details: Entity<DetailsPanel>,
        cx: &mut Context<Self>,
    ) -> Self {
        // The task beside the workspace, the panel's view, and the "..." menu of its task.
        let observe = Dashboard::redraw_on(&dashboard, &[Part::Tasks, Part::View], cx);
        DetailsRail {
            dashboard,
            details,
            _observe: observe,
        }
    }

    fn button(
        &self,
        view: DetailsView,
        picked: Option<DetailsView>,
        count: Option<usize>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, icon, label, command) = view_button(view);
        let on = picked == Some(view);
        let keys = command.and_then(shortcut_of);
        let badge = changes_badge(count);
        let tip = view_tip(label, badge);
        let button = IconButton::new(id, icon)
            .shape(IconButtonShape::Wide)
            .size(ButtonSize::Large)
            .width(px(RAIL_BUTTON))
            // A bit larger than Zed's 16 px, as the owner asked (T-0183).
            .icon_size(IconSize::Custom(rems_from_px(RAIL_ICON)))
            .icon_color(if on { Color::Default } else { Color::Muted })
            // Grey, not the accent: the pill alone shows the picked view (T-0171).
            .selected_icon_color(Color::Default)
            .toggle_state(on)
            .map(|button| match keys {
                Some(keys) => button.tooltip(tip_with_keys(tip, keys)),
                None => button.tooltip(Tooltip::text(tip)),
            })
            .on_click(cx.listener(move |rail, _, _, cx| {
                rail.dashboard
                    .update(cx, |dashboard, cx| dashboard.rail_pick(view, cx));
            }));
        let badge = badge.map(|count| {
            // At the button's outer corner, so it does not cover the icon.
            div()
                .absolute()
                .top(px(-3.))
                .right(px(-4.))
                .px_1()
                .rounded_full()
                .bg(cx.theme().colors().element_background)
                .border_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new(count.to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Warning),
                )
        });
        div()
            .relative()
            .child(button)
            .children(badge)
            .into_any_element()
    }
}

impl Render for DetailsRail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let dashboard = self.dashboard.read(cx);
        let Some(task) = dashboard.details_task() else {
            return div().into_any_element();
        };
        let views = details_tabs(
            task.workspace.is_some(),
            dashboard.details_beside_workspace(),
        );
        let picked = dashboard
            .details_shown()
            .then(|| dashboard.details_view_now());
        let count = dashboard.open_changes_of(&task.id);
        let buttons: Vec<AnyElement> = views
            .into_iter()
            .map(|view| {
                let count = (view == DetailsView::Changes).then_some(count).flatten();
                self.button(view, picked, count, cx)
            })
            .collect();
        let more = self
            .details
            .update(cx, |panel, cx| panel.rail_more_button(cx));
        let colors = cx.theme().colors();
        v_flex()
            .id("details-rail")
            .w(px(RAIL_WIDTH))
            .h_full()
            .flex_none()
            .overflow_hidden()
            .items_center()
            .gap_1p5()
            .py_1p5()
            .bg(colors.panel_background)
            .border_l_1()
            .border_color(colors.border)
            .children(buttons)
            .child(div().flex_1())
            .children(more)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_changes_badge_only_while_files_wait_to_be_committed() {
        assert_eq!(changes_badge(None), None);
        assert_eq!(changes_badge(Some(0)), None);
        assert_eq!(changes_badge(Some(1)), Some(1));
        assert_eq!(changes_badge(Some(57)), Some(57));
    }

    #[test]
    fn says_the_count_in_the_tooltip_of_changes() {
        assert_eq!(view_tip("Changes", Some(1)), "Changes, 1 file to commit");
        assert_eq!(view_tip("Changes", Some(57)), "Changes, 57 files to commit");
        assert_eq!(view_tip("History", None), "History");
    }
}
