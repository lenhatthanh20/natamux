//! The summary strip under the panel's header on Progress (T-0258): it stays while the flow
//! scrolls. The pull request or "No PR yet" on the left, the cost and the tokens on the right.

use gpui::AnyElement;
use natamux_proto::tasks::task::Task;
use ui::{Indicator, Tooltip, prelude::*};

use super::DetailsPanel;
use crate::app::links::open_link;
use crate::components::button_style::{button_icon, quiet_button};
use crate::tasks::summary_rules::{NO_PR, PrFacts, summary_facts};
use crate::tasks::table_rules::ChecksTone;

const COST_TIP: &str = "Estimated at API list prices. A subscription is billed by its plan.";

impl DetailsPanel {
    pub(super) fn summary_strip(&self, task: &Task, cx: &App) -> AnyElement {
        let usage = self.dashboard.read(cx).data.usage.get(&task.id);
        let facts = summary_facts(task, usage);
        let left = match facts.pr {
            Some(pr) => pull_request(pr),
            None => h_flex()
                .gap_1()
                .child(button_icon("git-pull-request"))
                .child(small(NO_PR.into(), Color::Muted))
                .into_any_element(),
        };
        let cost = facts.cost.map(|cost| {
            div()
                .id("details-summary-cost")
                .flex_none()
                .tooltip(Tooltip::text(COST_TIP))
                .child(small(cost, Color::Default))
        });
        let tokens = facts.tokens.map(|tokens| small(tokens, Color::Muted));
        h_flex()
            .id("details-summary")
            .flex_none()
            .w_full()
            .h(px(28.))
            .gap_3()
            .px_2()
            .bg(cx.theme().colors().tab_bar_background)
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(div().flex_1().min_w_0().overflow_hidden().child(left))
            .children(cost)
            .children(tokens)
            .into_any_element()
    }
}

fn small(text: String, color: Color) -> Label {
    Label::new(text).size(LabelSize::Small).color(color)
}

/// "#569" as a link, then the dot and "open · checks pass" once GitHub answered.
fn pull_request(pr: PrFacts) -> AnyElement {
    let url = pr.url.clone();
    let link = quiet_button("details-summary-pr", pr.label)
        .label_size(LabelSize::Small)
        .start_icon(button_icon("git-pull-request"))
        .tooltip(Tooltip::text(pr.url))
        .on_click(move |_, _, cx| open_link(&url, cx));
    let status = pr.status.map(|(text, tone)| {
        let color = match tone {
            ChecksTone::Passing => Color::Success,
            ChecksTone::Failing => Color::Error,
            ChecksTone::Pending => Color::Warning,
            ChecksTone::None => Color::Muted,
        };
        h_flex()
            .min_w_0()
            .gap_1()
            .child(Indicator::dot().color(color))
            .child(small(text, Color::Muted).truncate())
    });
    h_flex()
        .min_w_0()
        .gap_1()
        .child(link)
        .children(status)
        .into_any_element()
}
