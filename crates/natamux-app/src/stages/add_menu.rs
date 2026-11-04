//! The `+ Add` dropdown of the stage list header (docs/ui-spec.md 4.31): one button whose menu
//! opens the Add stage, Custom stage, Add branch and Side by side dialogs.

use gpui::{ClickEvent, Entity, FocusHandle};
use ui::{ContextMenu, prelude::*};

use super::pipeline_editor::PipelineEditor;
use super::{add_stage_dialog, branch_dialog, custom_stage_dialog, group_dialog};
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

type Open = fn(&Entity<PipelineEditor>, &mut Window, &mut App);

/// The menu items, in order, with the dialog each opens.
const ITEMS: [(&str, Open); 4] = [
    ("Built-in stage\u{2026}", add_stage_dialog::open),
    ("Custom stage\u{2026}", custom_stage_dialog::open),
    ("Branch\u{2026}", branch_dialog::open_add),
    ("Side by side\u{2026}", group_dialog::open_new),
];

/// The `+ Add ▾` button; `focus` makes it a stop of the Tab ring.
pub fn add_button(pipeline: &Entity<PipelineEditor>, focus: &FocusHandle) -> Button {
    let pipeline = pipeline.clone();
    Button::new("add-step", "Add")
        .start_icon(Icon::from_path(icon_path("plus")).size(IconSize::Small))
        .end_icon(Icon::new(IconName::ChevronDown).size(IconSize::XSmall))
        .style(ButtonStyle::Outlined)
        .label_size(LabelSize::Small)
        .track_focus(focus)
        .on_click(move |event: &ClickEvent, window, cx| open_menu(&pipeline, event, window, cx))
}

fn open_menu(
    pipeline: &Entity<PipelineEditor>,
    event: &ClickEvent,
    window: &mut Window,
    cx: &mut App,
) {
    let pipeline = pipeline.clone();
    let menu = ContextMenu::build(window, cx, move |menu, _, _| {
        ITEMS.iter().fold(menu, |menu, (label, open)| {
            let (pipeline, open) = (pipeline.clone(), *open);
            menu.entry(*label, None, move |window, cx| open(&pipeline, window, cx))
        })
    });
    MenuLayer::open(menu, event.position(), window, cx);
}
