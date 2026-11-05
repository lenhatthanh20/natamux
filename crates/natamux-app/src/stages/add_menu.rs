//! The "Add" button of the Stages group (layout-b 5.2.4): its menu opens the Add stage, Custom
//! stage, Add branch and Side by side dialogs.

use gpui::{ClickEvent, Entity, FocusHandle};
use ui::{ContextMenu, ContextMenuEntry, prelude::*};

use super::pipeline_editor::PipelineEditor;
use super::{add_stage_dialog, branch_dialog, custom_stage_dialog, group_dialog};
use crate::components::button_style::{action_button, button_icon};
use crate::components::icons::icon_path;
use crate::components::menu::MenuLayer;

type Open = fn(&Entity<PipelineEditor>, &mut Window, &mut App);

/// The menu items, in order, with their icon and the dialog each opens.
const ITEMS: [(&str, &str, Open); 4] = [
    ("Built-in stage\u{2026}", "bot", add_stage_dialog::open),
    ("Custom stage\u{2026}", "pencil", custom_stage_dialog::open),
    ("Branch\u{2026}", "diamond", branch_dialog::open_add),
    ("Side by side\u{2026}", "columns-3", group_dialog::open_new),
];

/// The `+ Add ▾` button; `focus` makes it a stop of the Tab ring.
pub fn add_button(pipeline: &Entity<PipelineEditor>, focus: &FocusHandle) -> Button {
    let pipeline = pipeline.clone();
    action_button("add-step", "Add")
        .start_icon(button_icon("plus"))
        .end_icon(
            Icon::new(IconName::ChevronDown)
                .size(IconSize::XSmall)
                .color(Color::Muted),
        )
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
        ITEMS.iter().fold(menu, |menu, (label, icon, open)| {
            let (pipeline, open) = (pipeline.clone(), *open);
            menu.item(
                ContextMenuEntry::new(*label)
                    .custom_icon_path(icon_path(icon))
                    .icon_position(IconPosition::Start)
                    .icon_color(Color::Muted)
                    .handler(move |window, cx| open(&pipeline, window, cx)),
            )
        })
    });
    MenuLayer::open(menu, event.position(), window, cx);
}
