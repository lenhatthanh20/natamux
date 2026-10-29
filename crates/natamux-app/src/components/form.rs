//! Form parts shared by dialogs and the edit form: a labelled field, a bordered text box, and a
//! text editor with a placeholder.

use editor::Editor;
use gpui::Focusable;
use gpui::{Entity, FontWeight};
use language::language_settings::SoftWrap;
use ui::prelude::*;

use super::tab_order;

pub fn field(label: impl Into<SharedString>, control: impl IntoElement) -> impl IntoElement {
    v_flex()
        .gap_1()
        .child(
            Label::new(label)
                .size(LabelSize::Small)
                .weight(FontWeight::MEDIUM)
                .color(Color::Muted),
        )
        .child(control)
}

pub fn input_box(editor: &Entity<Editor>, cx: &App) -> impl IntoElement {
    tab_order::push(&editor.focus_handle(cx));
    let colors = cx.theme().colors();
    div()
        .px_2p5()
        .py_1p5()
        .rounded_md()
        .border_1()
        .border_color(colors.border)
        .bg(colors.editor_background)
        .child(editor.clone())
}

/// One line, or `lines` (min, max) of text that grows with what is typed.
pub fn text_editor(
    placeholder: &str,
    lines: Option<(usize, usize)>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<Editor> {
    cx.new(|cx| {
        let mut editor = match lines {
            Some((min, max)) => {
                let mut editor = Editor::auto_height(min, max, window, cx);
                // A textarea wraps long lines; Zed's default setting does not.
                editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
                editor
            }
            None => Editor::single_line(window, cx),
        };
        editor.set_placeholder_text(placeholder, window, cx);
        editor
    })
}
