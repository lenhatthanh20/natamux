//! The agent of a task or a terminal: its icon, or a generic bot for an agent with no icon of
//! its own (`components/AgentBadge.tsx`). The icons keep their own colours, so they are images,
//! not tinted SVGs.

use gpui::img;
use ui::{Tooltip, prelude::*};

use crate::agents::agent_choices::{agent_icon_path, agent_label};
use crate::components::icons::icon_path;

/// The icon of an agent at 12 px.
#[derive(IntoElement)]
pub struct AgentIcon {
    agent: Option<String>,
}

impl AgentIcon {
    pub fn new(agent: Option<String>) -> Self {
        AgentIcon { agent }
    }
}

impl RenderOnce for AgentIcon {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let light = cx.theme().appearance.is_light();
        match agent_icon_path(self.agent.as_deref(), light) {
            Some(path) => img(SharedString::from(path))
                .flex_none()
                .size(rems_from_px(12.0_f32))
                .into_any_element(),
            None => Icon::from_path(icon_path("bot"))
                .size(IconSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
        }
    }
}

/// The agent of a terminal pane: its icon and name in a bordered chip. A pane narrower than 28rem
/// shows only the icon, as a name cut to one letter says nothing; the name stays in the tooltip.
#[derive(IntoElement)]
pub struct AgentBadge {
    agent: String,
    icon_only: bool,
}

/// `@min-[28rem]` of the web's badge.
pub const NAME_FROM_PANE_WIDTH: f32 = 448.;

impl AgentBadge {
    pub fn new(agent: String) -> Self {
        AgentBadge {
            agent,
            icon_only: false,
        }
    }

    pub fn icon_only(mut self, icon_only: bool) -> Self {
        self.icon_only = icon_only;
        self
    }
}

/// The padding around a grid of panes and the gap between them (8 in both grids).
const GRID_SPACE: f32 = 8.;

/// A pane is narrow when its own width, without the grid's padding and gaps, is under 28rem.
pub fn narrow_pane(width: Option<f32>, columns: usize) -> bool {
    let columns = columns.max(1) as f32;
    width.is_some_and(|width| {
        let pane = (width - 2. * GRID_SPACE - (columns - 1.) * GRID_SPACE) / columns;
        pane < NAME_FROM_PANE_WIDTH
    })
}

impl RenderOnce for AgentBadge {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let label = agent_label(Some(&self.agent));
        h_flex()
            .id(SharedString::from(format!("agent-badge-{}", self.agent)))
            .flex_none()
            .gap_1()
            .px_1p5()
            .rounded_sm()
            .border_1()
            .border_color(cx.theme().colors().border)
            .child(AgentIcon::new(Some(self.agent)))
            .when(!self.icon_only, |this| {
                this.child(
                    Label::new(label.clone())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted)
                        .single_line(),
                )
            })
            .tooltip(Tooltip::text(label))
    }
}

/// An agent's icon and name, for a table cell or a board card (`AgentName`).
pub fn agent_name(agent: &str, size: LabelSize) -> impl IntoElement {
    h_flex()
        .min_w_0()
        .gap_1p5()
        .child(AgentIcon::new(Some(agent.to_owned())))
        .child(
            Label::new(agent_label(Some(agent)))
                .size(size)
                .color(Color::Muted)
                .truncate(),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pane_narrower_than_28rem_shows_only_the_agent_icon() {
        // 2 columns of 900: (900 - 16 - 8) / 2 = 438.
        assert!(narrow_pane(Some(900.), 2));
        assert!(!narrow_pane(Some(930.), 2));
        assert!(!narrow_pane(Some(464.), 1));
        assert!(narrow_pane(Some(463.), 1));
        assert!(!narrow_pane(None, 3));
    }
}
