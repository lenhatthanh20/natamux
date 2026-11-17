//! `TextBox`: the one place that builds and drives the app's text inputs (plan 6.3.1). It wraps
//! the own `TextInput` (also the one of a code view).
//! Callers read and write through these methods and hear changes as `TextEvent`s.

use std::sync::Arc;

#[cfg(test)]
use gpui::EntityInputHandler;
use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, FocusHandle, Focusable, IntoElement,
    Subscription, TextStyleRefinement, Window,
};

use crate::text_view::text_input::TextInput;

/// What a box tells its owner. Both come as effects, after the change, never inside it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextEvent {
    /// The text changed (typing, paste, undo, `set_text`, `insert_at_caret`, `clear`).
    Edited,
    /// The box lost the focus.
    Blurred,
}

#[derive(Clone)]
pub struct TextBox {
    input: Entity<TextInput>,
}

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

    /// A full text view in the buffer font: wrapped text only, no gutter. The app builds the Docs
    /// boxes as code views (`CodeView::plain`); the tests of every box kind use this one.
    #[cfg_attr(not(test), allow(dead_code))]
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
        Self { input }
    }
}

impl TextBox {
    /// The box of a code view's input.
    pub(super) fn of_input(input: &Entity<TextInput>) -> Self {
        Self {
            input: input.clone(),
        }
    }

    /// The text, with the marked text of an input method in it.
    pub fn text(&self, cx: &App) -> String {
        self.input.read(cx).text().to_string()
    }

    /// An input method is composing text in the box: its keys stay with it.
    pub fn is_composing(&self, _: &mut Window, cx: &mut App) -> bool {
        self.input.read(cx).is_composing()
    }

    pub fn is_focused(&self, window: &Window, cx: &App) -> bool {
        self.focus_handle(cx).is_focused(window)
    }

    pub fn set_text(&self, text: impl Into<Arc<str>>, _: &mut Window, cx: &mut App) {
        let text = text.into();
        self.input.update(cx, |input, cx| input.set_text(&text, cx));
    }

    /// Changes the text into `text` with the fewest edits: the caret, the scroll and the undo
    /// history stay.
    pub fn replace_minimal(&self, text: &str, _: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.replace_minimal(text, cx));
    }

    pub fn clear(&self, _: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.set_text("", cx));
    }

    /// Puts `text` over the selection, or at the caret.
    pub fn insert_at_caret(&self, text: &str, _: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.insert_at_caret(text, cx));
    }

    pub fn select_all(&self, _: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.select_everything(cx));
    }

    pub fn move_to_end(&self, _: &mut Window, cx: &mut App) {
        self.input.update(cx, |input, cx| input.caret_to_end(cx));
    }

    pub fn set_placeholder(&self, text: &str, _: &mut Window, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_placeholder(text, cx));
    }

    /// Shows one `*` for each char; `text()` still gives the real text.
    pub fn set_masked(&self, masked: bool, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_masked(masked, cx));
    }

    /// No edits from the keys, the clipboard or an input method; the selection and Copy work.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_read_only(&self, read_only: bool, cx: &mut App) {
        self.input.update(cx, |input, cx| {
            input.set_read_only(read_only);
            cx.notify();
        });
    }

    /// A font family and size in px for the fields that need one (mono ids, commands, branches),
    /// in the input text style.
    pub fn set_style(&self, style: TextStyleRefinement, cx: &mut App) {
        self.input
            .update(cx, |input, cx| input.set_style(style, cx));
    }

    /// Hears the box's `TextEvent`s; the handler also gets the box.
    pub fn subscribe<T: 'static>(
        &self,
        cx: &mut Context<T>,
        mut handler: impl FnMut(&mut T, &TextBox, TextEvent, &mut Context<T>) + 'static,
    ) -> Subscription {
        cx.subscribe(&self.input, move |this, input, event: &TextEvent, cx| {
            let text_box = TextBox { input };
            handler(this, &text_box, *event, cx);
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
            &self.input,
            window,
            move |this, input, event: &TextEvent, window, cx| {
                let text_box = TextBox {
                    input: input.clone(),
                };
                handler(this, &text_box, *event, window, cx);
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
        cx.observe(&self.input, move |this, input, cx| {
            let text_box = TextBox { input };
            handler(this, &text_box, cx);
        })
    }

    /// The selection in UTF-16 units, as the platform's input method reads it.
    #[cfg(test)]
    pub fn selected_range(&self, window: &mut Window, cx: &mut App) -> std::ops::Range<usize> {
        let selection = self
            .input
            .update(cx, |input, cx| input.selected_text_range(false, window, cx));
        selection.map_or(0..0, |selection| selection.range)
    }

    /// The own input inside, for tests that call the input method on it.
    #[cfg(test)]
    pub fn editor(&self) -> &Entity<TextInput> {
        &self.input
    }
}

impl Focusable for TextBox {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.focus_handle(cx)
    }
}

impl IntoElement for TextBox {
    type Element = AnyElement;

    fn into_element(self) -> Self::Element {
        self.input.into_any_element()
    }
}
