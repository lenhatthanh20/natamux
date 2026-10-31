//! The JSON tab of Project settings (`JsonTab.tsx`): the draft text in a full Zed editor, with the
//! error while the text cannot be read and the problems of the pipeline above it, so they show
//! without scrolling (the web puts them under its shorter textarea). The editor and the draft
//! follow each other: a user edit sets the draft text, a text set from outside (Stages, Issues,
//! Import, Reset, Reload) goes into the same buffer as an edit of the changed parts.

use editor::{CurrentLineHighlight, Editor, EditorEvent, MultiBuffer};
use gpui::{Entity, FocusHandle, Focusable, Subscription};
use language::Buffer;
use natamux_proto::stages::pipeline_rules::IssueLevel;
use ui::prelude::*;

use super::settings_tab_rules::{json_alert, problem_line};
use crate::components::colors::signal;
use crate::git::diff_editors::plain_switches;
use crate::git::languages::language_for;
use crate::stages::kind_icon::problem_icon;
use crate::stages::pipeline_editor::PipelineEditor;

/// 22 rows of 20 px, and the padding of the box.
const EDITOR_HEIGHT: f32 = 22. * 20. + 16.;

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

    #[cfg(test)]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
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
        let warning = signal(cx).fg;
        let problems: Vec<(String, bool)> = pipeline
            .issues()
            .iter()
            .map(|issue| {
                (
                    problem_line(issue.stage.as_deref(), &issue.message),
                    issue.level == IssueLevel::Error,
                )
            })
            .collect();
        let colors = cx.theme().colors();
        let border = if error.is_some() {
            cx.theme().status().error_border
        } else {
            colors.border
        };
        v_flex()
            .gap_2()
            .children(error.map(|text| {
                h_flex()
                    .gap_1p5()
                    .items_start()
                    .child(
                        div()
                            .mt_0p5()
                            .child(problem_icon().size(IconSize::XSmall).color(Color::Error)),
                    )
                    .child(Label::new(text).size(LabelSize::Small).color(Color::Error))
            }))
            .when(!problems.is_empty(), |this| {
                this.child(v_flex().gap_0p5().children(problems.into_iter().map(
                    |(text, is_error)| {
                        let color = if is_error {
                            Color::Error
                        } else {
                            Color::Custom(warning)
                        };
                        Label::new(text).size(LabelSize::Small).color(color)
                    },
                )))
            })
            .child(
                div()
                    .h(px(EDITOR_HEIGHT))
                    .w_full()
                    .rounded_md()
                    .border_1()
                    .border_color(border)
                    .bg(colors.editor_background)
                    .overflow_hidden()
                    .child(self.editor.clone()),
            )
    }
}
