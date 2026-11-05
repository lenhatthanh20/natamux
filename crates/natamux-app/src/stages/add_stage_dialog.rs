//! The Add stage sheet (layout-b 5.4): the stage picker of the catalog stages. Up, Down and Enter
//! work from the search field; adding a stage closes the sheet and opens the new stage's sheet.

use gpui::{
    DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, KeystrokeEvent, Subscription,
};
use ui::prelude::*;

use super::pipeline_editor::PipelineEditor;
use super::stage_picker::{StageAdded, StagePicker};
use crate::components::dialog::{DialogFrame, DialogLayer, DialogView, cancel_button};
use crate::components::menu::press_of;

/// Opens the dialog with the focus in its search field.
pub fn open(pipeline: &Entity<PipelineEditor>, window: &mut Window, cx: &mut App) {
    let pipeline = pipeline.clone();
    let dialog = cx.new(|cx| AddStageDialog::new(pipeline, window, cx));
    let search = dialog.read(cx).picker.read(cx).search_focus(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&search, cx);
}

pub struct AddStageDialog {
    picker: Entity<StagePicker>,
    focus: FocusHandle,
    _added: Subscription,
    _keys: Subscription,
}

impl AddStageDialog {
    fn new(pipeline: Entity<PipelineEditor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let picker = cx.new(|cx| StagePicker::new(pipeline, window, cx));
        let added = cx.subscribe(&picker, |_, _, _: &StageAdded, cx| cx.emit(DismissEvent));
        AddStageDialog {
            _keys: Self::watch_keys(picker.clone(), cx),
            picker,
            focus: cx.focus_handle(),
            _added: added,
        }
    }

    /// Up, Down and Enter in the search field move in the list and add, before the field's own
    /// bindings take them.
    fn watch_keys(picker: Entity<StagePicker>, cx: &mut Context<Self>) -> Subscription {
        cx.intercept_keystrokes(move |event: &KeystrokeEvent, window, cx| {
            if !picker.read(cx).search_focus(cx).is_focused(window) {
                return;
            }
            let press = press_of(&event.keystroke);
            if press.secondary || press.other || press.shift {
                return;
            }
            let by = match press.key {
                "up" => -1,
                "down" => 1,
                "enter" => 0,
                _ => return,
            };
            if StagePicker::composing(&picker, window, cx) {
                return;
            }
            cx.stop_propagation();
            let picker = picker.clone();
            // Adding closes the dialog, which drops this listener: apply after the key.
            window.defer(cx, move |_, cx| {
                picker.update(cx, |picker, cx| {
                    if by == 0 {
                        picker.add_active(cx);
                    } else {
                        picker.move_active(by, cx);
                    }
                })
            });
        })
    }
}

impl EventEmitter<DismissEvent> for AddStageDialog {}

impl Focusable for AddStageDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for AddStageDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        vec![self.picker.read(cx).search_focus(cx)]
    }

    /// Mod+Enter adds the active stage, as Enter does.
    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.picker.update(cx, |picker, cx| picker.add_active(cx));
    }
}

impl Render for AddStageDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let frame = DialogFrame::new(
            "Add stage",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .sheet()
        .child(self.picker.clone())
        .footer(cancel_button(
            "Cancel",
            &self.focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
