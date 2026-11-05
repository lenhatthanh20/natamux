//! The General tab of Project settings (layout-b 5.1): the Agents, Git and GitHub issue groups
//! over the one draft. Each field draws its own row.

use ui::prelude::*;

use super::project_settings_page::StagesTab;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup};

/// The body of the General tab.
pub(super) fn render_general(stages: &StagesTab) -> AnyElement {
    let agents = SettingsCard::new(SeparatorInset::Tile)
        .child(stages.send_backs.clone())
        .child(stages.idle_pause.clone());
    let git = SettingsCard::new(SeparatorInset::Tile).child(stages.branch_name.clone());
    v_flex()
        .w_full()
        .child(SettingsGroup::new("Agents").child(agents))
        .child(SettingsGroup::new("Git").child(git))
        .child(stages.tabs.issues.clone())
        .into_any_element()
}
