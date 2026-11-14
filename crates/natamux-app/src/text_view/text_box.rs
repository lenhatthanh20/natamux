//! `TextBox`: the one place that builds and drives the app's text inputs (plan 6.3.1). It wraps
//! Zed's `Editor`, or the own `TextInput` with the `own-input` feature (S3.3); no caller changes.
//! Callers read and write through these methods and hear changes as `TextEvent`s.

use std::sync::Arc;

use editor::actions::{MoveToEnd, SelectAll};
use editor::{Editor, EditorEvent};
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, EntityInputHandler, FocusHandle, Focusable,
    IntoElement, Subscription, TextStyleRefinement, Window,
};
#[cfg(not(feature = "own-input"))]
use language::language_settings::SoftWrap;

#[cfg(not(feature = "own-input"))]
use crate::components::form::input_style;
#[cfg(feature = "own-input")]
use crate::text_view::text_input::TextInput;

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
    inner: Inner,
}

#[derive(Clone)]
enum Inner {
    Zed(Entity<Editor>),
    #[cfg(feature = "own-input")]
    Own(Entity<TextInput>),
}

#[cfg(feature = "own-input")]
impl TextBox {
    /// One line. Enter, Up and Down are left to the forms and lists around it.
    pub fn one_line(placeholder: &str, window: &mut Window, cx: &mut App) -> Self {
        Self::own(window, cx, |window, cx| {
            TextInput::one_line(placeholder, window, cx)
        })
    }

    /// A textarea of `min` to `max` lines that grows with the text and wraps long lines.
    pub fn area(
        placeholder: &str,
        (min, max): (usize, usize),
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        Self::own(window, cx, |window, cx| {
            TextInput::area(placeholder, min, max, window, cx)
        })
    }

    /// Many lines that fill their box and scroll inside, in the input text style: wrap on, no
    /// gutter, no line highlight.
    pub fn fill(placeholder: &str, window: &mut Window, cx: &mut App) -> Self {
        Self::own(window, cx, |window, cx| {
            TextInput::fill(placeholder, window, cx)
        })
    }

    /// A full text view in the buffer font: wrapped text only, no gutter. The Docs edit box and
    /// its source view (`read_only`).
    pub fn plain(read_only: bool, window: &mut Window, cx: &mut App) -> Self {
        Self::own(window, cx, |window, cx| {
            TextInput::plain(read_only, window, cx)
        })
    }

    fn own(
        window: &mut Window,
        cx: &mut App,
        make: impl FnOnce(&mut Window, &mut Context<TextInput>) -> TextInput,
    ) -> Self {
        let input = cx.new(|cx| make(window, cx));
        Self {
            inner: Inner::Own(input),
        }
    }
}

#[cfg(not(feature = "own-input"))]
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
        Self {
            inner: Inner::Zed(editor),
        }
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
            // An empty placeholder is not the same state as none: keep today's drawing.
            if !placeholder.is_empty() {
                editor.set_placeholder_text(placeholder, window, cx);
            }
            editor
        });
        Self {
            inner: Inner::Zed(editor),
        }
    }
}

impl TextBox {
    /// Wraps an editor that something else made, so the form parts can frame it. For the JSON
    /// tab until it has its own code view.
    pub fn of_editor(editor: &Entity<Editor>) -> Self {
        Self {
            inner: Inner::Zed(editor.clone()),
        }
    }

    /// The text, with the marked text of an input method in it.
    pub fn text(&self, cx: &App) -> String {
        match &self.inner {
            Inner::Zed(editor) => editor.read(cx).text(cx),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.read(cx).text().to_string(),
        }
    }

    /// An input method is composing text in the box: its keys stay with it.
    pub fn is_composing(&self, window: &mut Window, cx: &mut App) -> bool {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| {
                editor.marked_text_range(window, cx).is_some()
            }),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.read(cx).is_composing(),
        }
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }

    pub fn set_text(&self, text: impl Into<Arc<str>>, window: &mut Window, cx: &mut App) {
        let text = text.into();
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| editor.set_text(text, window, cx)),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.set_text(&text, cx)),
        }
    }

    /// Changes the text into `text` with the fewest edits: the caret, the scroll and the undo
    /// history stay. A wrapped `Editor` takes the whole text instead.
    #[cfg(feature = "own-input")]
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn replace_minimal(&self, text: &str, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| editor.set_text(text, window, cx)),
            Inner::Own(input) => input.update(cx, |input, cx| input.replace_minimal(text, cx)),
        }
    }

    pub fn clear(&self, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| editor.clear(window, cx)),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.set_text("", cx)),
        }
    }

    /// Puts `text` over the selection, or at the caret.
    pub fn insert_at_caret(&self, text: &str, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| editor.insert(text, window, cx)),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.insert_at_caret(text, cx)),
        }
    }

    pub fn select_all(&self, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => {
                editor.update(cx, |editor, cx| editor.select_all(&SelectAll, window, cx))
            }
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.select_everything(cx)),
        }
    }

    pub fn move_to_end(&self, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => {
                editor.update(cx, |editor, cx| editor.move_to_end(&MoveToEnd, window, cx))
            }
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.caret_to_end(cx)),
        }
    }

    pub fn set_placeholder(&self, text: &str, window: &mut Window, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| {
                editor.set_placeholder_text(text, window, cx)
            }),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.set_placeholder(text, cx)),
        }
    }

    /// Shows one `*` for each char; `text()` still gives the real text.
    pub fn set_masked(&self, masked: bool, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| editor.set_masked(masked, cx)),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.set_masked(masked, cx)),
        }
    }

    /// No edits from the keys, the clipboard or an input method; the selection and Copy work.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_read_only(&self, read_only: bool, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, _| editor.set_read_only(read_only)),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| {
                input.set_read_only(read_only);
                cx.notify();
            }),
        }
    }

    /// A font family and size in px for the fields that need one (mono ids, commands, branches),
    /// in the input text style.
    pub fn set_style(&self, style: TextStyleRefinement, cx: &mut App) {
        match &self.inner {
            Inner::Zed(editor) => {
                editor.update(cx, |editor, _| editor.set_text_style_refinement(style))
            }
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.update(cx, |input, cx| input.set_style(style, cx)),
        }
    }

    /// Hears the box's `TextEvent`s; the handler also gets the box.
    pub fn subscribe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, TextEvent, &mut Context<T>) + 'static,
    ) -> Subscription {
        match &self.inner {
            Inner::Zed(editor) => {
                cx.subscribe(editor, move |this, editor, event: &EditorEvent, cx| {
                    if let Some(event) = TextEvent::of(event) {
                        let text_box = TextBox {
                            inner: Inner::Zed(editor),
                        };
                        handler(this, &text_box, event, cx);
                    }
                })
            }
            #[cfg(feature = "own-input")]
            Inner::Own(input) => cx.subscribe(input, move |this, input, event: &TextEvent, cx| {
                let text_box = TextBox {
                    inner: Inner::Own(input),
                };
                handler(this, &text_box, *event, cx);
            }),
        }
    }

    /// `subscribe`, with the window.
    pub fn subscribe_in<T: 'static>(
        &self,
        window: &mut Window,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, TextEvent, &mut Window, &mut Context<T>) + 'static,
    ) -> Subscription {
        match &self.inner {
            Inner::Zed(editor) => cx.subscribe_in(
                editor,
                window,
                move |this, editor, event: &EditorEvent, window, cx| {
                    if let Some(event) = TextEvent::of(event) {
                        let text_box = TextBox {
                            inner: Inner::Zed(editor.clone()),
                        };
                        handler(this, &text_box, event, window, cx);
                    }
                },
            ),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => cx.subscribe_in(
                input,
                window,
                move |this, input, event: &TextEvent, window, cx| {
                    let text_box = TextBox {
                        inner: Inner::Own(input.clone()),
                    };
                    handler(this, &text_box, *event, window, cx);
                },
            ),
        }
    }

    /// Runs the handler each time the box changes in any way (text, caret, focus), as
    /// `cx.observe` does.
    pub fn observe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, &mut Context<T>) + 'static,
    ) -> Subscription {
        match &self.inner {
            Inner::Zed(editor) => cx.observe(editor, move |this, editor, cx| {
                let text_box = TextBox {
                    inner: Inner::Zed(editor),
                };
                handler(this, &text_box, cx);
            }),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => cx.observe(input, move |this, input, cx| {
                let text_box = TextBox {
                    inner: Inner::Own(input),
                };
                handler(this, &text_box, cx);
            }),
        }
    }

    /// The selection in UTF-16 units, as the platform's input method reads it.
    #[cfg(test)]
    pub fn selected_range(&self, window: &mut Window, cx: &mut App) -> std::ops::Range<usize> {
        let selection = match &self.inner {
            Inner::Zed(editor) => editor.update(cx, |editor, cx| {
                editor.selected_text_range(false, window, cx)
            }),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => {
                input.update(cx, |input, cx| input.selected_text_range(false, window, cx))
            }
        };
        selection.map_or(0..0, |selection| selection.range)
    }

    /// The editor inside, for tests that call the input method on it.
    #[cfg(all(test, not(feature = "own-input")))]
    pub fn editor(&self) -> &Entity<Editor> {
        match &self.inner {
            Inner::Zed(editor) => editor,
        }
    }

    /// The own input inside, for tests that call the input method on it. The tests name it
    /// `editor()` on both builds, so they run unchanged.
    #[cfg(all(test, feature = "own-input"))]
    pub fn editor(&self) -> &Entity<TextInput> {
        match &self.inner {
            Inner::Own(input) => input,
            Inner::Zed(_) => unreachable!("a box built by one_line, area, fill or plain"),
        }
    }
}

impl Focusable for TextBox {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match &self.inner {
            Inner::Zed(editor) => editor.focus_handle(cx),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.focus_handle(cx),
        }
    }
}

impl IntoElement for TextBox {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        match self.inner {
            Inner::Zed(editor) => editor.into_any_element(),
            #[cfg(feature = "own-input")]
            Inner::Own(input) => input.into_any_element(),
        }
    }
}
