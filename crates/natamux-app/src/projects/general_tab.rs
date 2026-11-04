//! The General tab of Project settings: the Project fields (send backs, idle pause, branch name)
//! and the GitHub issues choice, as two sections over the one draft.

use ui::prelude::*;

use super::project_settings_page::StagesTab;

/// A section: a small heading over its content.
fn section(title: &'static str, content: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_2()
        .child(
            Label::new(title)
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::SEMIBOLD),
        )
        .child(content)
}

/// The body of the General tab.
pub(super) fn render_general(stages: &StagesTab) -> AnyElement {
    let project = v_flex()
        .gap_3()
        .child(stages.send_backs.clone())
        .child(stages.idle_pause.clone())
        .child(stages.branch_name.clone());
    v_flex()
        .gap_5()
        .child(section("Project", project))
        .child(section("GitHub issues", stages.tabs.issues.clone()))
        .into_any_element()
}
