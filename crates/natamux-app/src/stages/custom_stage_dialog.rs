//! The Custom stage dialog (docs/ui-spec.md 4.31): name, id (from the name until typed), kind and
//! output file of a stage of your own. Add stage writes it before the release and picks it; the
//! rules are `custom_stage_problems`, shown once the person tried or typed.

use editor::{Editor, EditorEvent};
use gpui::{ClickEvent, DismissEvent, Entity, EventEmitter, FocusHandle, Focusable, Subscription};
use ui::{ButtonLike, prelude::*};

use super::editor_catalog::{
    CUSTOM_KINDS, CustomKind, CustomStageInput, add_custom_stage, custom_stage_problems, form_id,
    id_from_name, visible_problems,
};
use super::editor_labels::kind_label;
use super::pipeline_editor::PipelineEditor;
use super::stage_fields_view::{Choice, error_line, help_line, open_choices, select};
use crate::components::dialog::{
    DialogFrame, DialogLayer, DialogView, cancel_button, primary_button,
};
use crate::components::form::{field, input_box, text_editor};

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
        let name = text_editor("", None, window, cx);
        let id = text_editor("", None, window, cx);
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
            kind_focus: cx.focus_handle(),
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

    fn kind_menu(&self, cx: &mut Context<Self>) -> ButtonLike {
        let kind = self.kind;
        let this = cx.weak_entity();
        select("custom-kind", kind_label(kind.stage_kind()))
            .track_focus(&self.kind_focus)
            .on_click(move |event: &ClickEvent, window, cx| {
                let choices = CUSTOM_KINDS.iter().map(|choice| {
                    let (this, choice) = (this.clone(), *choice);
                    Choice::new(
                        kind_label(choice.stage_kind()),
                        Some(choice == kind),
                        move |_, cx| {
                            this.update(cx, |this, cx| {
                                this.kind = choice;
                                cx.notify();
                            })
                            .ok();
                        },
                    )
                });
                open_choices(choices.collect(), event.position(), window, cx);
            })
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
        let kind = self.kind_menu(cx);
        let app: &App = cx;
        let group = |label: &'static str, editor: &Entity<Editor>, problem: Option<String>| {
            v_flex()
                .gap_1p5()
                .child(field(label, input_box(editor, app)))
                .children(problem.map(error_line))
        };
        let frame = DialogFrame::new(
            "Add a custom stage",
            cx.listener(|_, _, _, cx| cx.emit(DismissEvent)),
        )
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
        .child(group("Name", &self.name, shown.name))
        .child(
            h_flex()
                .items_start()
                .gap(rems_from_px(12.0_f32))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(group("Id", &self.id, shown.id)),
                )
                .child(div().min_w_0().flex_1().child(field("Kind", kind))),
        )
        .when(agent, |this| {
            this.child(group("Output file", &self.output, shown.output))
        })
        .child(help_line(
            "It waits for your approval. You can change that.",
        ));
        div()
            .key_context("Dialog")
            .track_focus(&self.focus)
            .child(frame)
    }
}
