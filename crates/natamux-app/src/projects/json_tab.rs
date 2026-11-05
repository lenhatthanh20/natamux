//! The JSON tab of Project settings (`JsonTab.tsx`, layout-b 5.8): the error alert while the text
//! cannot be read, the problems of the pipeline, then the draft text in a full Zed editor. The
//! editor and the draft follow each other: a user edit sets the draft text, a text set from
//! outside (Stages, Issues, Import, Reset, Reload) goes into the same buffer as an edit of the
//! changed parts.

use editor::{CurrentLineHighlight, Editor, EditorEvent, MultiBuffer};
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use language::Buffer;
use natamux_proto::stages::pipeline_rules::IssueLevel;
use ui::prelude::*;

use super::settings_tab_rules::{json_alert, problem_line};
use crate::components::form::SettingsInput;
use crate::components::settings_card::SettingsGroup;
use crate::components::settings_tile::{AlertKind, SettingsAlert};
use crate::git::diff_editors::plain_switches;
use crate::git::languages::language_for;
use crate::stages::pipeline_editor::PipelineEditor;

/// The box of the editor (layout-b 5.8).
const EDITOR_HEIGHT: f32 = 456.;

const NOTE: &str = "The same draft as the other tabs. Nothing is saved until you click Save.";

pub struct JsonTab {
    pipeline: Entity<PipelineEditor>,
    editor: Entity<Editor>,
    buffer: Entity<Buffer>,
    _subscriptions: Vec<Subscription>,
    _set: Option<gpui::Task<()>>,
}

impl JsonTab {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = pipeline.read(cx).text().to_owned();
        let buffer = cx.new(|cx| Buffer::local(text, cx));
        color_as_json(&buffer, cx);
        let multi = cx.new(|cx| MultiBuffer::singleton(buffer.clone(), cx));
        let editor = cx.new(|cx| {
            let mut editor = Editor::for_multibuffer(multi, None, window, cx);
            plain_switches(&mut editor, false, window, cx);
            editor.set_show_gutter(false, cx);
            editor.set_current_line_highlight(Some(CurrentLineHighlight::None));
            editor
        });
        let subscriptions = vec![
            cx.subscribe(&editor, |this, editor, event: &EditorEvent, cx| {
                if matches!(event, EditorEvent::BufferEdited) {
                    let text = editor.read(cx).text(cx);
                    this.pipeline
                        .update(cx, |pipeline, cx| pipeline.set_text(&text, cx));
                }
            }),
            cx.observe(&pipeline, |this, _, cx| {
                this.follow_draft(cx);
                cx.notify();
            }),
        ];
        JsonTab {
            pipeline,
            editor,
            buffer,
            _subscriptions: subscriptions,
            _set: None,
        }
    }

    /// A text set from outside goes into the same buffer: only the changed parts are edited, so
    /// the cursor and the scroll stay. The edit that follows sets the same text on the draft,
    /// which ignores it.
    fn follow_draft(&mut self, cx: &mut Context<Self>) {
        let text = self.pipeline.read(cx).text().to_owned();
        if self.editor.read(cx).text(cx) == text {
            return;
        }
        let buffer = self.buffer.clone();
        let diff = buffer.read(cx).diff(text, cx);
        self._set = Some(cx.spawn(async move |_, cx| {
            let diff = diff.await;
            buffer.update(cx, |buffer, cx| {
                buffer.apply_diff(diff, cx);
            });
        }));
    }

    /// The stop of the Tab ring.
    pub fn stop(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl Focusable for JsonTab {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.stop(cx)
    }
}

/// The buffer takes the JSON grammar once it is loaded, for colours.
fn color_as_json(buffer: &Entity<Buffer>, cx: &mut Context<JsonTab>) {
    let Some(language) = language_for(".natamux.json", cx) else {
        return;
    };
    let buffer = buffer.clone();
    cx.spawn(async move |_, cx| {
        if let Some(language) = language.await {
            buffer.update(cx, |buffer, cx| buffer.set_language(Some(language), cx));
        }
    })
    .detach();
}

impl Render for JsonTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let pipeline = self.pipeline.read(cx);
        let error = pipeline.parse_error().map(json_alert);
        let problems: Vec<_> = pipeline
            .issues()
            .iter()
            .map(|issue| {
                let color = if issue.level == IssueLevel::Error {
                    Color::Error
                } else {
                    Color::Warning
                };
                let text = problem_line(issue.stage.as_deref(), &issue.message);
                Label::new(text).size(LabelSize::Small).color(color)
            })
            .collect();
        let has_problems = !problems.is_empty();
        let editor = SettingsInput::new(&self.editor)
            .area()
            .radius(8.)
            .height(EDITOR_HEIGHT)
            .invalid(error.is_some());
        v_flex()
            .w_full()
            .children(error.map(|text| {
                SettingsAlert::new(AlertKind::Error, "The JSON cannot be read").text(text)
            }))
            .when(has_problems, |tab| {
                tab.child(v_flex().pt(rems_from_px(10.0_f32)).children(problems))
            })
            .child(
                SettingsGroup::new(".natamux.json")
                    .child(editor)
                    .footnote(NOTE),
            )
    }
}
