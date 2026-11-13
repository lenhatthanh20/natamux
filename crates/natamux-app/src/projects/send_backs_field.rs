//! The Agent send backs field of Project settings: `agents.maxSendBacks` of the draft, a number
//! or empty for no limit. A good edit writes the draft at once; a bad one shows its error and
//! leaves the draft as it was. A change of the draft from elsewhere (the JSON tab, Import, Reset)
//! shows in the field unless it has the focus, so typing never moves the cursor.

use editor::{Editor, EditorEvent};
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use ui::prelude::*;

use super::settings_tab_rules::{SEND_BACKS_HINT, SEND_BACKS_LABEL, send_backs_field};
use crate::components::colors::TileColor;
use crate::components::form::{SettingsInput, text_editor};
use crate::components::settings_card::SettingsRow;
use crate::components::settings_controls::changed_mark;
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::stages::pipeline_editor::PipelineEditor;

pub struct SendBacksField {
    pipeline: Entity<PipelineEditor>,
    editor: Entity<Editor>,
    error: Option<&'static str>,
    _subscriptions: Vec<Subscription>,
}

fn shown(most: Option<usize>) -> String {
    most.map(|most| most.to_string()).unwrap_or_default()
}

impl SendBacksField {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = text_editor("No limit", None, window, cx);
        let text = shown(pipeline.read(cx).max_send_backs());
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
        SendBacksField {
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
        match send_backs_field(text) {
            Ok(most) => {
                self.error = None;
                self.pipeline.update(cx, |pipeline, cx| {
                    if pipeline.max_send_backs() != most {
                        pipeline.set_max_send_backs(most, cx);
                    }
                });
            }
            Err(error) => self.error = Some(error),
        }
        cx.notify();
    }

    /// Reset icon: an empty field is the default; the edit event writes the draft.
    fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editor
            .update(cx, |editor, cx| editor.set_text("", window, cx));
    }

    fn sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.editor.focus_handle(cx).is_focused(window) {
            return;
        }
        let want = self.pipeline.read(cx).max_send_backs();
        if send_backs_field(&self.editor.read(cx).text(cx)) == Ok(want) {
            return;
        }
        self.error = None;
        self.editor
            .update(cx, |editor, cx| editor.set_text(shown(want), window, cx));
    }
}

impl Render for SendBacksField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let changed = !self.editor.read(cx).text(cx).trim().is_empty();
        let mark = changed_mark(
            "send-backs",
            changed,
            move |window, cx| {
                this.update(cx, |this, cx| this.reset(window, cx)).ok();
            },
            cx,
        );
        let tile = SettingsTile::icon(
            Icon::new(IconName::RotateCcw),
            TileColor::Blue,
            TileSize::Row,
        );
        let input = SettingsInput::new(&self.editor)
            .width(120.)
            .invalid(self.error.is_some());
        SettingsRow::new("send-backs-field", SEND_BACKS_LABEL)
            .lead(tile)
            .help(SEND_BACKS_HINT)
            .error(self.error)
            .control(mark)
            .control(input)
    }
}

#[cfg(test)]
#[path = "send_backs_field_tests.rs"]
mod tests;
