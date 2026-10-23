//! The shown file of the Docs tab: its header, the stale line, and the body (edit box, source,
//! Markdown, or the empty state).

use editor::Editor;
use language::language_settings::SoftWrap;
use ui::{Button, ButtonSize, ButtonStyle, prelude::*};

use super::docs_rules::{EMPTY_TEXT, STALE_LINE, is_stale};
use super::docs_tab::DocsTab;
use crate::components::colors::signal;
use crate::components::segmented::Segmented;
use crate::components::skeleton::{Loading, SkeletonList};

impl DocsTab {
    pub(super) fn source_view(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let editor = match &self.source {
            Some((editor, _)) => editor.clone(),
            None => cx.new(|cx| {
                let mut editor = Editor::multi_line(window, cx);
                plain(&mut editor, window, cx);
                editor.set_read_only(true);
                editor
            }),
        };
        if self.source.as_ref().is_none_or(|(_, shown)| shown != text) {
            editor.update(cx, |editor, cx| {
                editor.set_text(text.to_owned(), window, cx)
            });
            self.source = Some((editor.clone(), text.to_owned()));
        }
        div()
            .size_full()
            .min_h_0()
            .p_3()
            .child(editor)
            .into_any_element()
    }

    pub(super) fn header(&self, name: &str, cx: &mut Context<Self>) -> AnyElement {
        let draft = self.store.read(cx).doc(&self.task_id, name).is_some();
        let failed = self.text_failed(name);
        let busy = self.saving;
        let row = h_flex()
            .h(rems_from_px(40.0_f32))
            .flex_none()
            .gap_2()
            .px_3()
            .border_b_1()
            .border_color(cx.theme().colors().border)
            .child(
                Label::new(name.to_owned())
                    .size(LabelSize::XSmall)
                    .buffer_font(cx)
                    .color(Color::Muted),
            )
            .child(div().flex_1());
        if draft {
            return row
                .child(
                    Button::new("docs-cancel", "Cancel")
                        .label_size(LabelSize::XSmall)
                        .size(ButtonSize::Compact)
                        .style(ButtonStyle::Subtle)
                        .on_click(cx.listener(|this, _, window, cx| this.cancel(window, cx))),
                )
                .child(
                    Button::new("docs-save", "Save")
                        .label_size(LabelSize::XSmall)
                        .size(ButtonSize::Compact)
                        .style(ButtonStyle::Filled)
                        .loading(busy)
                        .disabled(busy)
                        .on_click(cx.listener(|this, _, window, cx| this.save(window, cx))),
                )
                .into_any_element();
        }
        let source = self.dashboard.read(cx).view().docs_source;
        let dashboard = self.dashboard.clone();
        let off = self.text_of(name).is_none() && self.modified_of(name).is_some();
        let own = name.to_owned();
        row.when(!failed, |row| {
            row.child(Segmented::new(
                "docs-view",
                &self.view_focus,
                [
                    ("Rendered".into(), IconName::FileMarkdown),
                    ("Source".into(), IconName::FileCode),
                ],
                usize::from(source),
                move |index, _, cx| {
                    dashboard.update(cx, |d, cx| d.set_docs_source(index == 1, cx));
                },
            ))
        })
        .child(
            Button::new("docs-edit", "Edit")
                .label_size(LabelSize::XSmall)
                .size(ButtonSize::Compact)
                .style(ButtonStyle::Outlined)
                .disabled(off)
                .on_click(cx.listener(move |this, _, _, cx| this.start_edit(&own, cx))),
        )
        .into_any_element()
    }

    pub(super) fn text_failed(&self, name: &str) -> bool {
        self.text_error.as_ref().is_some_and(|(own, _)| own == name)
    }

    pub(super) fn body(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(edit) = self.edit.as_ref().filter(|edit| edit.name == name) {
            return div()
                .size_full()
                .min_h_0()
                .p_3()
                .child(edit.editor.clone())
                .into_any_element();
        }
        if let Some((_, message)) = self.text_error.as_ref().filter(|(own, _)| own == name) {
            return div()
                .p_4()
                .child(Label::new(message.clone()).color(Color::Error))
                .into_any_element();
        }
        let Some(text) = self.text_of(name).map(str::to_owned) else {
            if self.modified_of(name).is_none() {
                return div().into_any_element();
            }
            return Loading::new("docs-loading", true, SkeletonList::new(6, 28.))
                .content(div())
                .into_any_element();
        };
        if self.dashboard.read(cx).view().docs_source {
            return self.source_view(&text, window, cx);
        }
        self.markdown.set_text(&text, cx);
        self.markdown.view(window, cx)
    }

    pub(super) fn section(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stale = self
            .store
            .read(cx)
            .doc(&self.task_id, name)
            .is_some_and(|draft| is_stale(draft, self.modified_of(name).as_deref()));
        let line = stale.then(|| {
            div()
                .flex_none()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new(STALE_LINE)
                        .size(LabelSize::XSmall)
                        .color(Color::Custom(signal(cx).fg)),
                )
        });
        v_flex()
            .flex_1()
            .min_w_0()
            .min_h_0()
            .child(self.header(name, cx))
            .children(line)
            .child(div().flex_1().min_h_0().child(self.body(name, window, cx)))
            .into_any_element()
    }

    pub(super) fn empty(&self, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .p_4()
            .gap_3()
            .items_start()
            .child(Label::new(EMPTY_TEXT).color(Color::Muted))
            .child(
                Button::new("docs-write-first", "Write context.md")
                    .label_size(LabelSize::XSmall)
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Outlined)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.start_new = true;
                        this.start_edit(super::docs_rules::FIRST_FILE, cx);
                    })),
            )
            .child(self.attachment_rows(true, cx))
            .into_any_element()
    }
}

/// The switches of the editors of this tab: text only, wrapped, no gutter.
pub(super) fn plain(editor: &mut Editor, window: &mut Window, cx: &mut Context<Editor>) {
    editor.set_show_gutter(false, cx);
    editor.set_show_line_numbers(false, cx);
    editor.set_show_runnables(false, cx);
    editor.set_show_breakpoints(false, cx);
    editor.set_show_code_actions(false, cx);
    editor.set_show_bookmarks(false, cx);
    editor.set_show_wrap_guides(false, cx);
    editor.set_show_indent_guides(false, cx);
    editor.set_show_edit_predictions(Some(false), window, cx);
    editor.set_minimap_visibility(editor::MinimapVisibility::Disabled, window, cx);
    editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
    // Mono 12, as the source view and the editor tab.
    editor.set_text_style_refinement(gpui::TextStyleRefinement {
        font_size: Some(px(12.).into()),
        ..Default::default()
    });
}
