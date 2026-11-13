//! The Branch name field of Project settings: `git.branchName` of the draft, a pattern or empty
//! for the default name. It behaves as the send backs field: a good edit writes the draft at once; a bad one shows its error and
//! leaves the draft as it was. A change of the draft from elsewhere (the JSON tab, Import, Reset)
//! shows in the field unless it has the focus, so typing never moves the cursor.

use editor::{Editor, EditorEvent};
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use ui::prelude::*;

use super::settings_tab_rules::{
    BRANCH_NAME_HINT, BRANCH_NAME_LABEL, BRANCH_NAME_PLACEHOLDER, branch_name_field,
};
use crate::components::colors::TileColor;
use crate::components::form::{SettingsInput, text_editor};
use crate::components::settings_card::SettingsRow;
use crate::components::settings_controls::changed_mark;
use crate::components::settings_tile::{SettingsTile, TileSize};
use crate::stages::pipeline_editor::PipelineEditor;

pub struct BranchNameField {
    pipeline: Entity<PipelineEditor>,
    editor: Entity<Editor>,
    error: Option<String>,
    _subscriptions: Vec<Subscription>,
}

fn shown(name: Option<String>) -> String {
    name.unwrap_or_default()
}

impl BranchNameField {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let editor = text_editor(BRANCH_NAME_PLACEHOLDER, None, window, cx);
        let text = shown(pipeline.read(cx).branch_name());
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
        BranchNameField {
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
        match branch_name_field(text) {
            Ok(name) => {
                self.error = None;
                self.pipeline.update(cx, |pipeline, cx| {
                    if pipeline.branch_name() != name {
                        pipeline.set_branch_name(name.as_deref(), cx);
                    }
                });
            }
            Err(problem) => self.error = Some(problem),
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
        let want = self.pipeline.read(cx).branch_name();
        if branch_name_field(&self.editor.read(cx).text(cx)) == Ok(want.clone()) {
            return;
        }
        self.error = None;
        self.editor
            .update(cx, |editor, cx| editor.set_text(shown(want), window, cx));
    }
}

impl Render for BranchNameField {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let this = cx.weak_entity();
        let changed = !self.editor.read(cx).text(cx).trim().is_empty();
        let mark = changed_mark(
            "branch-name",
            changed,
            move |window, cx| {
                this.update(cx, |this, cx| this.reset(window, cx)).ok();
            },
            cx,
        );
        let tile = SettingsTile::icon(
            Icon::new(IconName::GitBranch),
            TileColor::Green,
            TileSize::Row,
        );
        let input = SettingsInput::new(&self.editor)
            .width(260.)
            .invalid(self.error.is_some());
        SettingsRow::new("branch-name-field", BRANCH_NAME_LABEL)
            .lead(tile)
            .help(BRANCH_NAME_HINT)
            .error(self.error.clone())
            .control(mark)
            .control(input)
    }
}
