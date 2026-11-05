//! The General tab of Project settings: one card with the Agents, Git and GitHub issue sections
//! over the one draft.

use ui::prelude::*;

use super::project_settings_page::StagesTab;

/// The card holds at most this width.
const CARD_MAX_WIDTH: f32 = 640.;

/// A section: the title in small upper-case, then its content; sections split by a border.
fn section(
    title: &'static str,
    last: bool,
    content: impl IntoElement,
    cx: &App,
) -> impl IntoElement {
    v_flex()
        .px(rems_from_px(14.0_f32))
        .py(rems_from_px(10.0_f32))
        .when(!last, |this| {
            this.border_b_1()
                .border_color(cx.theme().colors().border_variant)
        })
        .child(
            div().mb(rems_from_px(8.0_f32)).child(
                Label::new(title.to_uppercase())
                    .size(LabelSize::Custom(rems_from_px(11_f32)))
                    .weight(gpui::FontWeight::MEDIUM)
                    .color(Color::Placeholder),
            ),
        )
        .child(content)
}

/// The body of the General tab.
pub(super) fn render_general(stages: &StagesTab, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    let agents = v_flex()
        .gap(rems_from_px(10.0_f32))
        .child(stages.send_backs.clone())
        .child(stages.idle_pause.clone());
    let card = v_flex()
        .w_full()
        .max_w(rems_from_px(CARD_MAX_WIDTH))
        .border_1()
        .border_color(colors.border_variant)
        .rounded(px(8.))
        .bg(colors.panel_background)
        .child(section("Agents", false, agents, cx))
        .child(section("Git", false, stages.branch_name.clone(), cx))
        .child(section(
            "Open a GitHub issue for a task",
            true,
            stages.tabs.issues.clone(),
            cx,
        ));
    div().child(card).into_any_element()
}
