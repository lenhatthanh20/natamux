//! `TextBox`: the one place that builds and drives the app's text inputs (plan 6.3.1). It wraps
//! Zed's `Editor` today; the own input replaces the inside later, and no caller changes. Callers
//! read and write through these methods and hear changes as `TextEvent`s.

use std::sync::Arc;

use editor::actions::{MoveToEnd, SelectAll};
use editor::{Editor, EditorEvent};
use gpui::{
    App, Context, Entity, FocusHandle, Focusable, IntoElement, Subscription, TextStyleRefinement,
    Window,
};
use language::language_settings::SoftWrap;

use crate::components::form::input_style;

/// What a box tells its owner. Both come as effects, after the change, never inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEvent {
    /// The text changed (typing, paste, undo, `set_text`, `insert_at_caret`, `clear`).
    Edited,
    /// The box lost the focus.
    Blurred,
}

impl TextEvent {
    fn of(event: &EditorEvent) -> Option<Self> {
        match event {
            EditorEvent::BufferEdited => Some(Self::Edited),
            EditorEvent::Blurred => Some(Self::Blurred),
            _ => None,
        }
    }
}

#[derive(Clone)]
pub struct TextBox {
    editor: Entity<Editor>,
}

impl TextBox {
    /// One line. Enter, Up and Down are left to the forms and lists around it.
    pub fn one_line(placeholder: &str, window: &mut Window, cx: &mut App) -> Self {
        Self::build(placeholder, window, cx, |window, cx| {
            Editor::single_line(window, cx)
        })
    }

    /// A textarea of `min` to `max` lines that grows with the text and wraps long lines.
    pub fn area(
        placeholder: &str,
        (min, max): (usize, usize),
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self::build(placeholder, window, cx, |window, cx| {
            let mut editor = Editor::auto_height(min, max, window, cx);
            // Zed's default setting does not wrap.
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor
        })
    }

    /// Many lines that fill their box and scroll inside, in the input text style: wrap on, no
    /// gutter, no line highlight.
    pub fn fill(placeholder: &str, window: &mut Window, cx: &mut App) -> Self {
        Self::build(placeholder, window, cx, |window, cx| {
            let mut editor = Editor::multi_line(window, cx);
            editor.set_show_gutter(false, cx);
            editor.set_show_line_numbers(false, cx);
            editor.set_show_wrap_guides(false, cx);
            editor.set_show_indent_guides(false, cx);
            editor.set_current_line_highlight(Some(editor::CurrentLineHighlight::None));
            editor.set_soft_wrap_mode(SoftWrap::EditorWidth, cx);
            editor
        })
    }

    /// A full text view in the buffer font: wrapped text only, no gutter. The Docs edit box and
    /// its source view (`read_only`).
    pub fn plain(read_only: bool, window: &mut Window, cx: &mut App) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = Editor::multi_line(window, cx);
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
            if read_only {
                editor.set_read_only(true);
            }
            editor
        });
        Self { editor }
    }

    fn build(
        placeholder: &str,
        window: &mut Window,
        cx: &mut App,
        make: impl FnOnce(&mut Window, &mut Context<Editor>) -> Editor,
    ) -> Self {
        let editor = cx.new(|cx| {
            let mut editor = make(window, cx);
            editor.set_text_style_refinement(input_style(None, None));
            editor.set_placeholder_text(placeholder, window, cx);
            editor
        });
        Self { editor }
    }

    /// Wraps an editor that something else made, so the form parts can frame it. For the JSON
    /// tab until it has its own code view.
    pub fn of_editor(editor: &Entity<Editor>) -> Self {
        Self {
            editor: editor.clone(),
        }
    }

    /// The text, with the marked text of an input method in it.
    pub fn text(&self, cx: &App) -> String {
        self.editor.read(cx).text(cx)
    }

    /// An input method is composing text in the box: its keys stay with it.
    pub fn is_composing(&self, window: &mut Window, cx: &mut App) -> bool {
        self.editor.update(cx, |editor, cx| {
            editor.marked_text_range(window, cx).is_some()
        })
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }

    pub fn set_text(&self, text: impl Into<Arc<str>>, window: &mut Window, cx: &mut App) {
        let text = text.into();
        self.editor
            .update(cx, |editor, cx| editor.set_text(text, window, cx));
    }

    pub fn clear(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.clear(window, cx));
    }

    /// Puts `text` over the selection, or at the caret.
    pub fn insert_at_caret(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.insert(text, window, cx));
    }

    pub fn select_all(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.select_all(&SelectAll, window, cx));
    }

    pub fn move_to_end(&self, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.move_to_end(&MoveToEnd, window, cx));
    }

    pub fn set_placeholder(&self, text: &str, window: &mut Window, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.set_placeholder_text(text, window, cx));
    }

    /// Shows one `*` for each char; `text()` still gives the real text.
    pub fn set_masked(&self, masked: bool, cx: &mut App) {
        self.editor
            .update(cx, |editor, cx| editor.set_masked(masked, cx));
    }

    /// A font family and size in px for the fields that need one (mono ids, commands, branches),
    /// in the input text style.
    pub fn set_style(&self, style: TextStyleRefinement, cx: &mut App) {
        self.editor
            .update(cx, |editor, _| editor.set_text_style_refinement(style));
    }

    /// Hears the box's `TextEvent`s; the handler also gets the box.
    pub fn subscribe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, TextEvent, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.subscribe(&self.editor, move |this, editor, event: &EditorEvent, cx| {
            if let Some(event) = TextEvent::of(event) {
                handler(this, &TextBox { editor }, event, cx);
            }
        })
    }

    /// `subscribe`, with the window.
    pub fn subscribe_in<T: 'static>(
        &self,
        window: &mut Window,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, TextEvent, &mut Window, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.subscribe_in(
            &self.editor,
            window,
            move |this, editor, event: &EditorEvent, window, cx| {
                if let Some(event) = TextEvent::of(event) {
                    handler(this, &TextBox { editor: editor.clone() }, event, window, cx);
                }
            },
        )
    }

    /// Runs the handler each time the box changes in any way (text, caret, focus), as
    /// `cx.observe` does.
    pub fn observe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.observe(&self.editor, move |this, editor, cx| {
            handler(this, &TextBox { editor }, cx);
        })
    }

    /// The editor inside, for tests that call the input method on it.
    #[cfg(test)]
    pub fn editor(&self) -> &Entity<Editor> {
        &self.editor
    }
}

impl Focusable for TextBox {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.editor.focus_handle(cx)
    }
}

impl IntoElement for TextBox {
    type Element = <Entity<Editor> as IntoElement>::Element;

    fn into_element(self) -> Self::Element {
        self.editor.into_element()
    }
}
