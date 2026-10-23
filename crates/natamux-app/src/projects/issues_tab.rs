//! The Issues tab of Project settings (`IssuesTab.tsx`): when a task opens a GitHub issue, as four
//! radio rows over `tracker.createIssue` of the draft. The group is one tab stop; the arrows move
//! the choice. It is off (but still shows the value) while `tracker.type` is `none`.

use gpui::{Entity, FocusHandle, Focusable, KeyDownEvent};
use ui::prelude::*;

use super::settings_tab_rules::{ISSUES_OFF_NOTE, choice_index, issues_off, step_choice};
use super::settings_text::CREATE_ISSUE_OPTIONS;
use crate::components::colors::signal;
use crate::stages::editor_draft::DEFAULT_CREATE_ISSUE;
use crate::stages::pipeline_editor::PipelineEditor;

pub struct IssuesTab {
    pipeline: Entity<PipelineEditor>,
    focus: FocusHandle,
    _observe: gpui::Subscription,
}

impl IssuesTab {
    pub fn new(pipeline: Entity<PipelineEditor>, cx: &mut Context<Self>) -> Self {
        let observe = cx.observe(&pipeline, |_, _, cx| cx.notify());
        IssuesTab {
            pipeline,
            focus: cx.focus_handle().tab_stop(true),
            _observe: observe,
        }
    }

    pub fn stop(&self) -> FocusHandle {
        self.focus.clone()
    }

    fn pick(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some((value, _)) = CREATE_ISSUE_OPTIONS.get(index) else {
            return;
        };
        self.pipeline
            .update(cx, |pipeline, cx| pipeline.set_create_issue(value, cx));
    }

    fn radio(&self, checked: bool, off: bool, cx: &App) -> impl IntoElement + use<> {
        let colors = cx.theme().colors();
        let ring = if checked {
            colors.text_accent
        } else {
            colors.border
        };
        div()
            .mt(rems_from_px(2.0_f32))
            .size(rems_from_px(14.0_f32))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .border_1()
            .border_color(ring)
            .when(checked, |this| {
                this.child(
                    div()
                        .size(rems_from_px(6.0_f32))
                        .rounded_full()
                        .bg(colors.text_accent),
                )
            })
            .when(off, |this| this.opacity(0.5))
    }

    fn option_row(
        &self,
        index: usize,
        selected: usize,
        off: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (value, words) = CREATE_ISSUE_OPTIONS[index];
        let colors = cx.theme().colors();
        let color = if off { Color::Disabled } else { Color::Default };
        h_flex()
            .min_h(rems_from_px(28.0_f32))
            .flex_wrap()
            .gap_2()
            .child(
                h_flex()
                    .id(SharedString::from(format!("create-issue-{value}")))
                    .items_start()
                    .gap_2()
                    .when(!off, |this| {
                        this.cursor_pointer()
                            .on_click(cx.listener(move |this, _, window, cx| {
                                window.focus(&this.focus, cx);
                                this.pick(index, cx);
                            }))
                    })
                    .child(self.radio(index == selected, off, cx))
                    .child(Label::new(words).size(LabelSize::Small).color(color)),
            )
            .when(value == DEFAULT_CREATE_ISSUE, |this| {
                this.child(
                    div()
                        .px_1p5()
                        .rounded_sm()
                        .bg(colors.element_background)
                        .child(
                            Label::new("Default")
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        ),
                )
            })
    }
}

impl Focusable for IssuesTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// A line of the list under the choices: pieces side by side, `code` ones in the code font.
fn bullet(pieces: Vec<(&'static str, bool)>, cx: &App) -> impl IntoElement + use<> {
    h_flex()
        .items_start()
        .gap_1p5()
        .child(
            Label::new("\u{2022}")
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .child(
            h_flex()
                .flex_wrap()
                .children(pieces.into_iter().map(|(text, code)| {
                    let label = Label::new(text).size(LabelSize::XSmall).color(Color::Muted);
                    if code { label.buffer_font(cx) } else { label }
                })),
        )
}

impl Render for IssuesTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tracker = self.pipeline.read(cx).tracker();
        let off = issues_off(&tracker);
        let selected = choice_index(&tracker.create_issue);
        let warning = signal(cx).fg;
        let mut options = Vec::new();
        for index in 0..CREATE_ISSUE_OPTIONS.len() {
            options.push(self.option_row(index, selected, off, cx).into_any_element());
        }
        v_flex()
            .gap_3()
            .child(
                v_flex()
                    .id("create-issue-group")
                    .gap_1p5()
                    .track_focus(&self.focus)
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if off {
                            return;
                        }
                        let Some(next) = step_choice(selected, &event.keystroke.key) else {
                            return;
                        };
                        cx.stop_propagation();
                        if next != selected {
                            this.pick(next, cx);
                        }
                    }))
                    .child(
                        div().mb_1p5().child(
                            Label::new("Open a GitHub issue for a task")
                                .size(LabelSize::Small)
                                .weight(gpui::FontWeight::SEMIBOLD),
                        ),
                    )
                    .children(options),
            )
            .when(off, |this| {
                this.child(
                    Label::new(ISSUES_OFF_NOTE)
                        .size(LabelSize::XSmall)
                        .color(Color::Custom(warning)),
                )
            })
            .child(
                v_flex()
                    .gap_0p5()
                    .child(bullet(
                        vec![
                            ("Opened with ", false),
                            ("gh issue create", true),
                            (" on GitHub, in this project.", false),
                        ],
                        cx,
                    ))
                    .child(bullet(vec![("Title: the task title.", false)], cx))
                    .child(bullet(
                        vec![
                            ("Body: the ", false),
                            ("spec.md", true),
                            (
                                " of the task, or the task notes without one, and a line \
                                 \"Made by Natamux for T-0001.\"",
                                false,
                            ),
                        ],
                        cx,
                    )),
            )
    }
}
