//! The GitHub issues group of the General tab (`IssuesTab.tsx`, layout-b 5.1): when a task opens a
//! GitHub issue, as four radio rows over `tracker.createIssue` of the draft. The card is one tab
//! stop; the arrows move the choice. It is off (but still shows the value) while `tracker.type`
//! is `none`.

use gpui::{Entity, FocusHandle, Focusable, KeyDownEvent};
use ui::prelude::*;

use super::settings_tab_rules::{
    ISSUES_OFF_NOTE, IssueRow, choice_index, issue_rows, issues_off, step_choice,
};
use super::settings_text::CREATE_ISSUE_OPTIONS;
use crate::components::button_style::button_icon;
use crate::components::chips::label_chip;
use crate::components::settings_card::{
    SeparatorInset, SettingsCard, SettingsGroup, foot_bullets, foot_text,
};
use crate::components::settings_controls::radio;
use crate::components::text_size::row_text;
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

    /// A radio row: the radio, the label (with the Default chip), the note at the right.
    fn option_row(
        &self,
        index: usize,
        row: IssueRow,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let hover = cx.theme().colors().ghost_element_hover;
        h_flex()
            .id(SharedString::from(format!("create-issue-{}", row.value)))
            .w_full()
            .min_h(rems_from_px(36.0_f32))
            .px(rems_from_px(12.0_f32))
            .py(rems_from_px(6.0_f32))
            .gap(rems_from_px(16.0_f32))
            .when(row.disabled, |this| this.opacity(0.5))
            .when(!row.disabled, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        window.focus(&this.focus, cx);
                        this.pick(index, cx);
                    }))
            })
            .child(radio(row.selected, cx))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(rems_from_px(6.0_f32))
                    .child(Label::new(row.label).size(row_text()).truncate())
                    .when(row.is_default, |this| this.child(label_chip("Default"))),
            )
            .children(row.note.map(|note| {
                Label::new(note)
                    .size(LabelSize::Small)
                    .color(Color::Placeholder)
            }))
    }
}

impl Focusable for IssuesTab {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

/// A footnote line: pieces side by side, `code` ones in the code font.
fn foot_line(pieces: &[(&'static str, bool)], cx: &App) -> AnyElement {
    h_flex()
        .flex_wrap()
        .children(pieces.iter().map(|(text, code)| {
            let label = foot_text(*text);
            if *code { label.buffer_font(cx) } else { label }
        }))
        .into_any_element()
}

/// The warning line over the footnote list while issues are off.
fn off_line() -> impl IntoElement {
    h_flex()
        .gap(rems_from_px(6.0_f32))
        .mb(rems_from_px(4.0_f32))
        .child(
            button_icon("triangle-alert")
                .size(IconSize::XSmall)
                .color(Color::Warning),
        )
        .child(
            Label::new(ISSUES_OFF_NOTE)
                .size(LabelSize::Small)
                .color(Color::Warning),
        )
}

impl Render for IssuesTab {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let tracker = self.pipeline.read(cx).tracker();
        let off = issues_off(&tracker);
        let selected = choice_index(&tracker.create_issue);
        let mut card = SettingsCard::new(SeparatorInset::Tile);
        let rows = issue_rows(&tracker.create_issue, off);
        for (index, row) in rows.into_iter().enumerate() {
            card = card.child(self.option_row(index, row, cx));
        }
        let card = div()
            .id("create-issue-group")
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
            .child(card);
        let notes = [
            foot_line(
                &[
                    ("Runs ", false),
                    ("gh issue create", true),
                    (" in the project folder.", false),
                ],
                cx,
            ),
            foot_line(&[("The title is the task title.", false)], cx),
            foot_line(&[("The body is spec.md.", false)], cx),
        ];
        let group = SettingsGroup::new("Open a GitHub issue for a task").child(card);
        let group = if off {
            group.footnote_child(off_line())
        } else {
            group
        };
        group.footnote_child(foot_bullets(notes))
    }
}
