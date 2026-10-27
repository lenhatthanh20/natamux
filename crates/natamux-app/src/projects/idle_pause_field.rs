//! The idle pause field of Project settings: `agents.pauseIdleAfterMinutes` of the draft, 0 (off),
//! 5 to 1440, or empty for the default 30. It behaves as the send backs field: a good edit writes
//! the draft at once, a bad one shows its error, and a change from elsewhere shows in the field
//! unless it has the focus.

use editor::{Editor, EditorEvent};
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use natamux_proto::projects::repo::DEFAULT_PAUSE_IDLE_MINUTES;
use ui::prelude::*;

use super::settings_tab_rules::{IDLE_PAUSE_HINT, IDLE_PAUSE_LABEL, idle_pause_field};
use crate::components::form::{input_box, text_editor};
use crate::stages::pipeline_editor::PipelineEditor;
use crate::stages::stage_fields_view::{error_line, labelled};

pub struct IdlePauseField {
    pipeline: Entity<PipelineEditor>,
    editor: Entity<Editor>,
    error: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

fn shown(minutes: Option<u64>) -> String {
    minutes
        .map(|minutes| minutes.to_string())
        .unwrap_or_default()
}

impl IdlePauseField {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = DEFAULT_PAUSE_IDLE_MINUTES.to_string();
        let editor = text_editor(&placeholder, None, window, cx);
        let text = shown(pipeline.read(cx).pause_idle_minutes());
        editor.update(cx, |editor, cx| editor.set_text(text, window, cx));
        let edits = cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| {
            if matches!(event, EditorEvent::BufferEdited) {
                let text = editor.read(cx).text(cx);
                this.typed(&text, cx);
            }
        });
        let follow = cx.observe_in(&pipeline, window, |this, _, window, cx| {
            this.sync(window, cx);
        });
        IdlePauseField {
            pipeline,
            editor,
            error: None,
            _subscriptions: vec![edits, follow],
        }
    }

    pub fn stop(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }

    fn typed(&mut self, text: &str, cx: &mut Context<Self>) {
        match idle_pause_field(text) {
            Ok(minutes) => {
                self.error = None;
                self.pipeline.update(cx, |pipeline, cx| {
                    if pipeline.pause_idle_minutes() != minutes {
                        pipeline.set_pause_idle_minutes(minutes, cx);
                    }
                });
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.focus_handle(cx).is_focused(window) {
            return;
        }
        let want = self.pipeline.read(cx).pause_idle_minutes();
        if idle_pause_field(&self.editor.read(cx).text(cx)) == Ok(want) {
            return;
        }
        self.error = None;
        self.editor
            .update(cx, |editor, cx| editor.set_text(shown(want), window, cx));
    }
}

impl Render for IdlePauseField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let control = div()
            .w(rems_from_px(120.0_f32))
            .child(input_box(&self.editor, cx));
        v_flex()
            .id("idle-pause-field")
            .gap_1()
            .child(labelled(
                IDLE_PAUSE_LABEL,
                None,
                Some(IDLE_PAUSE_HINT.into()),
                control,
            ))
            .children(self.error.map(error_line))
    }
}
