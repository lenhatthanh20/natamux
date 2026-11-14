//! The Custom stage sheet (layout-b 5.5): name, id (from the name until typed), kind and output
//! file of a stage of your own. Add stage writes it before the release and opens its sheet; the
//! rules are `custom_stage_problems`, shown once the person tried or typed.

use editor::{Editor, EditorEvent};
use gpui::{DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription};
use ui::prelude::*;

use super::editor_catalog::{
    CUSTOM_KINDS, CustomKind, CustomStageInput, add_custom_stage, custom_stage_problems, form_id,
    id_from_name, visible_problems,
};
use super::pipeline_editor::PipelineEditor;
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::{SettingsInput, text_editor};
use crate::components::segmented::Segmented;
use crate::components::settings_card::{SeparatorInset, SettingsCard, SettingsGroup, SettingsRow};

/// Opens the dialog with the focus in Name.
pub fn open(pipeline: &Entity<PipelineEditor>, window: &mut Window, cx: &mut App) {
    let pipeline = pipeline.clone();
    let dialog = cx.new(|cx| CustomStageDialog::new(pipeline, window, cx));
    let name = dialog.read(cx).name.focus_handle(cx);
    DialogLayer::open(dialog, window, cx);
    window.focus(&name, cx);
}

pub struct CustomStageDialog {
    pipeline: Entity<PipelineEditor>,
    name: Entity<Editor>,
    id: Entity<Editor>,
    output: Entity<Editor>,
    /// The id typed by hand; None while it follows the name.
    own_id: Option<String>,
    kind: CustomKind,
    tried: bool,
    focus: FocusHandle,
    kind_focus: FocusHandle,
    cancel_focus: FocusHandle,
    submit_focus: FocusHandle,
    _edits: Vec<Subscription>,
}

impl CustomStageDialog {
    fn new(pipeline: Entity<PipelineEditor>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let name = text_editor("Changelog", None, window, cx);
        let id = text_editor("changelog", None, window, cx);
        let output = text_editor("notes.md", None, window, cx);
        let edits = vec![
            cx.subscribe_in(&name, window, |this, _, event: &EditorEvent, window, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.name_edited(window, cx);
                }
            }),
            cx.subscribe(&id, |this, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    this.id_edited(cx);
                }
            }),
            cx.subscribe(&output, |_, _, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    cx.notify();
                }
            }),
        ];
        CustomStageDialog {
            pipeline,
            name,
            id,
            output,
            own_id: None,
            kind: CustomKind::Agent,
            tried: false,
            focus: cx.focus_handle(),
            kind_focus: cx.focus_handle().tab_stop(true),
            cancel_focus: cx.focus_handle(),
            submit_focus: cx.focus_handle(),
            _edits: edits,
        }
    }

    /// The id follows the name until it is edited.
    fn name_edited(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.own_id.is_none() {
            let id = id_from_name(&self.name.read(cx).text(cx));
            self.id
                .update(cx, |editor, cx| editor.set_text(id, window, cx));
        }
        cx.notify();
    }

    fn id_edited(&mut self, cx: &mut Context<Self>) {
        let text = self.id.read(cx).text(cx);
        let followed = form_id(&self.name.read(cx).text(cx), self.own_id.as_deref());
        if text != followed {
            self.own_id = Some(text);
        }
        cx.notify();
    }

    fn input(&self, cx: &App) -> CustomStageInput {
        CustomStageInput {
            name: self.name.read(cx).text(cx),
            id: self.id.read(cx).text(cx),
            kind: self.kind,
            output: self.output.read(cx).text(cx),
        }
    }

    /// Agent, Testing or Manual.
    fn kind_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let this = cx.weak_entity();
        let options = [
            (SharedString::from("Agent"), IconName::Sparkle),
            (SharedString::from("Testing"), IconName::ToolHammer),
            (SharedString::from("Manual"), IconName::Person),
        ];
        let picked = CUSTOM_KINDS
            .iter()
            .position(|kind| *kind == self.kind)
            .unwrap_or(0);
        Segmented::new(
            "custom-kind",
            &self.kind_focus,
            options,
            picked,
            move |index, _, cx| {
                this.update(cx, |this, cx| {
                    if let Some(kind) = CUSTOM_KINDS.get(index) {
                        this.kind = *kind;
                        cx.notify();
                    }
                })
                .ok();
            },
        )
    }
}

impl EventEmitter<DismissEvent> for CustomStageDialog {}

impl Focusable for CustomStageDialog {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl DialogView for CustomStageDialog {
    fn tab_ring(&self, cx: &App) -> Vec<FocusHandle> {
        let mut ring = vec![
            self.name.focus_handle(cx),
            self.id.focus_handle(cx),
            self.kind_focus.clone(),
        ];
        if self.kind == CustomKind::Agent {
            ring.push(self.output.focus_handle(cx));
        }
        ring.extend([self.cancel_focus.clone(), self.submit_focus.clone()]);
        ring
    }

    fn submit(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        self.tried = true;
        let input = self.input(cx);
        let ready = custom_stage_problems(self.pipeline.read(cx).steps(), &input).is_empty();
        if ready {
            self.pipeline.update(cx, |pipeline, cx| {
                let steps = add_custom_stage(pipeline.steps(), &input);
                pipeline.edit(steps, cx);
                pipeline.select(&input.id, cx);
            });
            cx.emit(DismissEvent);
        }
        cx.notify();
    }
}

impl Render for CustomStageDialog {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let input = self.input(cx);
        let problems = custom_stage_problems(self.pipeline.read(cx).steps(), &input);
        let shown = visible_problems(&problems, &input, self.tried);
        let agent = self.kind == CustomKind::Agent;
        let kind = self.kind_tabs(cx);
        let mut rows = vec![
            SettingsRow::new("custom-name", "Name")
                .error(shown.name)
                .control(SettingsInput::new(&self.name).width(240.))
                .into_any_element(),
            SettingsRow::new("custom-id", "Id")
                .help("Follows the name until you edit it.")
                .error(shown.id)
                .control(SettingsInput::new(&self.id).width(240.))
                .into_any_element(),
            SettingsRow::new("custom-kind-row", "Kind")
                .control(kind)
                .into_any_element(),
        ];
        if agent {
            rows.push(
                SettingsRow::new("custom-output", "Output file")
                    .help("Optional.")
                    .error(shown.output)
                    .control(SettingsInput::new(&self.output).width(240.))
                    .into_any_element(),
            );
        }
        let group = SettingsGroup::new("Stage")
            .first(true)
            .child(SettingsCard::new(SeparatorInset::Text).children(rows))
            .footnote("It waits for your approval. You can change that.");
        let frame = DialogFrame::new(
            "Add a custom stage",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
        .sheet()
        .footer(cancel_button(
            "Cancel",
            &self.cancel_focus,
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        ))
        .footer(primary_button(
            "Add stage",
            &self.submit_focus,
            false,
            cx.listener(|this, _, window, cx| this.submit(window, cx)),
        ))
        .child(group);
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}

#[cfg(test)]
#[path = "custom_stage_dialog_tests.rs"]
mod tests;
