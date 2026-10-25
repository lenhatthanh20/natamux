//! The rendered view of a stage file (`Markdown.tsx`), on Zed's `markdown` crate: no HTML, no
//! Mermaid, no images, and only web and mail links open. All use of that crate stays in this file.

use gpui::{AnyElement, Entity};
use markdown::{
    CodeBlockRenderer, CopyButtonVisibility, Markdown, MarkdownElement, MarkdownFont,
    MarkdownOptions, MarkdownStyle, WrapButtonVisibility,
};
use ui::prelude::*;

use super::docs_rules::is_safe_link;

/// One `Markdown` entity per shown file; a new text goes in with `replace`.
pub struct MarkdownDoc {
    markdown: Entity<Markdown>,
}

impl MarkdownDoc {
    pub fn new(cx: &mut App) -> Self {
        let options = MarkdownOptions {
            parse_html: false,
            render_mermaid_diagrams: false,
            ..Default::default()
        };
        Self {
            markdown: cx.new(|cx| Markdown::new_with_options("".into(), None, None, options, cx)),
        }
    }

    /// The same text does nothing, so the view does not parse or redraw.
    pub fn set_text(&self, text: &str, cx: &mut App) {
        if self.markdown.read(cx).source().as_ref() == text {
            return;
        }
        let text = text.to_owned();
        self.markdown
            .update(cx, |markdown, cx| markdown.replace(text, cx));
    }

    pub fn view(&self, window: &Window, cx: &App) -> AnyElement {
        let style = MarkdownStyle::themed(MarkdownFont::Preview, window, cx);
        let body = MarkdownElement::new(self.markdown.clone(), style)
            .code_block_renderer(CodeBlockRenderer::Default {
                copy_button_visibility: CopyButtonVisibility::Hidden,
                wrap_button_visibility: WrapButtonVisibility::Hidden,
                border: true,
            })
            .on_url_click(|url, _, cx| {
                if is_safe_link(&url) {
                    cx.open_url(&url);
                }
            });
        div()
            .id("docs-markdown")
            .size_full()
            .min_h_0()
            .p_4()
            .overflow_y_scroll()
            .child(body)
            .into_any_element()
    }
}
