//! The rendered view of a stage file (`Markdown.tsx`). Until the app takes Zed's `markdown` crate
//! (asked of the owner), it shows the text as it is, wrapped and selectable.

use gpui::AnyElement;
use ui::prelude::*;

/// The shown text; a new text replaces it.
pub struct MarkdownDoc {
    text: SharedString,
}

impl MarkdownDoc {
    pub fn new(_: &mut App) -> Self {
        Self {
            text: SharedString::default(),
        }
    }

    pub fn set_text(&mut self, text: &str, _: &mut App) {
        if self.text.as_ref() != text {
            self.text = text.to_owned().into();
        }
    }

    pub fn view(&self, _: &Window, _: &App) -> AnyElement {
        div()
            .id("docs-markdown")
            .size_full()
            .min_h_0()
            .p_4()
            .overflow_y_scroll()
            .child(Label::new(self.text.clone()).size(LabelSize::Small))
            .into_any_element()
    }
}
