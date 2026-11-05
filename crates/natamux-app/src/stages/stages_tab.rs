//! The body of a project's Stages tab (layout-b 5.2): the "Pipeline" group with the flowchart in
//! its box, then the "Stages" group (`StageList`). The flowchart itself is `pipeline_preview`,
//! unchanged.

use gpui::{Entity, ScrollHandle};
use ui::prelude::*;

use super::pipeline_editor::PipelineEditor;
use super::pipeline_preview::pipeline_preview;
use super::stage_list::StageList;
use crate::components::settings_card::SettingsGroup;

/// Two parts, so the page can scroll to the second (the stage list).
pub fn stages_tab(
    editor: &Entity<PipelineEditor>,
    list: &Entity<StageList>,
    preview_scroll: &ScrollHandle,
    window: &mut Window,
    cx: &mut App,
) -> [AnyElement; 2] {
    let flow = pipeline_preview(editor, preview_scroll, window, cx);
    let pipeline = SettingsGroup::new("Pipeline").first(true).child(flow);
    [pipeline.into_any_element(), list.clone().into_any_element()]
}
