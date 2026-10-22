//! The agent of a task or a terminal: its icon, or a generic bot for an agent with no icon of
//! its own (`components/AgentBadge.tsx`). The icons keep their own colours, so they are images,
//! not tinted SVGs.

use gpui::img;
use ui::{Tooltip, prelude::*};

use crate::agents::agent_choices::{agent_icon_path, agent_label};

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
                .size(px(12.))
                .into_any_element(),
            None => Icon::from_path("icons/lucide/bot.svg")
                .size(IconSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
        }
    }
}

/// The agent of a terminal pane: its icon and name in a bordered chip.
#[derive(IntoElement)]
pub struct AgentBadge {
    agent: String,
}

impl AgentBadge {
    pub fn new(agent: String) -> Self {
        AgentBadge { agent }
    }
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
            .child(
                Label::new(label.clone())
                    .size(LabelSize::XSmall)
                    .color(Color::Muted)
                    .single_line(),
            )
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
