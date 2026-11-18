//! The JSON tab of Project settings (`JsonTab.tsx`, layout-b 5.8): the error alert while the text
//! cannot be read, the problems of the pipeline, then the draft text in a code view (JSON
//! colours, indent guides, no line highlight, wrapped). The view and the draft follow each other:
//! a user edit sets the draft text, a text set from outside (Stages, Issues, Import, Reset,
//! Reload) goes in as an edit of the changed parts only, so typed chars, the caret and the undo
//! history stay. The Copy button copies the draft as shown, also when it cannot be read.

use gpui::{ClickEvent, ClipboardItem, Entity, FocusHandle, Focusable, Subscription};
use natamux_proto::stages::pipeline_rules::IssueLevel;
use ui::{Tooltip, prelude::*};

use super::settings_tab_rules::{json_alert, problem_line};
use crate::components::form::SettingsInput;
use crate::components::settings_card::SettingsGroup;
use crate::components::settings_tile::{AlertKind, SettingsAlert};
use crate::components::toast_rules::ToastTone;
use crate::components::toasts::toast;
use crate::stages::pipeline_editor::PipelineEditor;
use crate::text_view::code_view::CodeView;
use crate::text_view::text_box::{TextBox, TextEvent};

/// The box of the editor (layout-b 5.8).
const EDITOR_HEIGHT: f32 = 456.;

/// The path the view takes its language from.
const PATH: &str = ".natamux.json";

const NOTE: &str = "The same draft as the other tabs. Nothing is saved until you click Save.";

pub struct JsonTab {
    pipeline: Entity<PipelineEditor>,
    code: TextBox,
    #[cfg(test)]
    view: CodeView,
    _subscriptions: Vec<Subscription>,
}

impl JsonTab {
    pub fn new(
        pipeline: Entity<PipelineEditor>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let text = pipeline.read(cx).text().to_owned();
        let view = CodeView::new(PATH, &text, false, window, cx);
        let code = view.text_box();
        let subscriptions = vec![
            code.subscribe(cx, |this, code, event, cx| {
                if event == TextEvent::Edited {
                    let text = code.text(cx);
                    this.pipeline
                        .update(cx, |pipeline, cx| pipeline.set_text(&text, cx));
                }
            }),
            cx.observe_in(&pipeline, window, |this, _, window, cx| {
                this.follow_draft(window, cx);
                cx.notify();
            }),
        ];
        JsonTab {
            pipeline,
            code,
            #[cfg(test)]
            view,
            _subscriptions: subscriptions,
        }
    }

    /// A text set from outside goes in as an edit of the changed parts only, so the caret and the
    /// scroll stay. The edit that follows sets the same text on the draft, which ignores it.
    fn follow_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.pipeline.read(cx).text().to_owned();
        if self.code.text(cx) != text {
            self.code.replace_minimal(&text, window, cx);
        }
    }

    /// The stop of the Tab ring.
    pub fn stop(&self, cx: &App) -> FocusHandle {
        self.code.focus_handle(cx)
    }

    #[cfg(test)]
    pub fn code_view(&self) -> &CodeView {
        &self.view
    }
}

impl Focusable for JsonTab {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.stop(cx)
    }
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
        let draft = self.pipeline.clone();
        let copy = div().debug_selector(|| "json-copy".into()).child(
            IconButton::new("json-copy", IconName::Copy)
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text("Copy JSON"))
                .on_click(move |_: &ClickEvent, _, cx| {
                    let text = draft.read(cx).text().to_owned();
                    cx.write_to_clipboard(ClipboardItem::new_string(text));
                    toast("Copied the JSON.", ToastTone::Info, cx);
                }),
        );
        let editor = SettingsInput::new(&self.code)
            .bar_edge()
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
                    .action(copy)
                    .child(div().debug_selector(|| "json-box".into()).child(editor))
                    .footnote(NOTE),
            )
    }
}

#[cfg(test)]
#[path = "json_tab_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "json_tab_copy_tests.rs"]
mod copy_tests;
