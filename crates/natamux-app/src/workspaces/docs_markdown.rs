//! The rendered view of a stage file (`Markdown.tsx`), on Zed's `markdown` crate: no HTML, no
//! Mermaid, no images, and only web and mail links open. All use of that crate stays in this file.

use gpui::{AnyElement, Entity, TextStyleRefinement};
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
        let mut style = MarkdownStyle::themed(MarkdownFont::Preview, window, cx);
        doc_text_sizes(&mut style);
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

/// The Preview style sizes text from the window rem (16 px body, 28 px h1); this puts it on the
/// app's scale (`text_size.rs`, 13 px body) with the web's heading steps. Rems keep the UI zoom.
fn doc_text_sizes(style: &mut MarkdownStyle) {
    let body = rems_from_px(13.0_f32);
    style.base_text_style.font_size = body.into();
    style.container_style.text.font_size = Some(body.into());
    style.code_block.text.font_size = Some(rems_from_px(13.0_f32).into());
    style.inline_code.font_size = Some(rems_from_px(12.0_f32).into());
    let levels = style
        .heading_level_styles
        .get_or_insert_with(Default::default);
    for (level, size) in [
        (&mut levels.h1, 20.0_f32),
        (&mut levels.h2, 16.0),
        (&mut levels.h3, 15.0),
        (&mut levels.h4, 13.0),
        (&mut levels.h5, 13.0),
        (&mut levels.h6, 12.0),
    ] {
        level
            .get_or_insert_with(TextStyleRefinement::default)
            .font_size = Some(rems_from_px(size).into());
    }
}

#[cfg(test)]
mod tests {
    use gpui::{AbsoluteLength, FontWeight};
    use markdown::HeadingLevelStyles;

    use super::*;

    fn size_px(size: f32) -> AbsoluteLength {
        rems_from_px(size).into()
    }

    fn heading_sizes(style: &MarkdownStyle) -> Vec<Option<AbsoluteLength>> {
        let Some(levels) = style.heading_level_styles.as_ref() else {
            return Vec::new();
        };
        [
            &levels.h1, &levels.h2, &levels.h3, &levels.h4, &levels.h5, &levels.h6,
        ]
        .into_iter()
        .map(|level| level.as_ref().and_then(|level| level.font_size))
        .collect()
    }

    #[test]
    fn puts_the_doc_on_the_app_text_scale() {
        let mut style = MarkdownStyle::default();
        doc_text_sizes(&mut style);
        assert_eq!(style.base_text_style.font_size, size_px(13.0));
        assert_eq!(style.container_style.text.font_size, Some(size_px(13.0)));
        assert_eq!(style.code_block.text.font_size, Some(size_px(13.0)));
        assert_eq!(style.inline_code.font_size, Some(size_px(12.0)));
        let expected: Vec<_> = [20.0, 16.0, 15.0, 13.0, 13.0, 12.0]
            .into_iter()
            .map(|size| Some(size_px(size)))
            .collect();
        assert_eq!(heading_sizes(&style), expected);
    }

    #[test]
    fn keeps_the_heading_weight_and_gives_the_same_sizes_twice() {
        let mut style = MarkdownStyle {
            heading_level_styles: Some(HeadingLevelStyles {
                h1: Some(TextStyleRefinement {
                    font_weight: Some(FontWeight::SEMIBOLD),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        };
        doc_text_sizes(&mut style);
        let once = heading_sizes(&style);
        doc_text_sizes(&mut style);
        assert_eq!(heading_sizes(&style), once);
        let h1 = style
            .heading_level_styles
            .as_ref()
            .and_then(|levels| levels.h1.as_ref());
        assert_eq!(h1.and_then(|h1| h1.font_weight), Some(FontWeight::SEMIBOLD));
    }
}
