//! What a blurred project shows in place of its private text (docs/ui-spec.md, "Blurred project"):
//! a picture of the text with a Gaussian blur, as the web's `filter: blur(5px)`. GPUI has no blur
//! filter, so its SVG renderer makes the picture (`blur_image.rs`), in the background. Until the
//! picture is there, and when it fails, the row draws a soft bar. The sharp text is never drawn.

use gpui::{AnyElement, BoxShadow, Font, FontWeight, img, point};
use ui::prelude::*;

use super::blur_image::{BlurCache, BlurKey, Entry, MAX_CHARS, Picture, ROOM};
use crate::app::app_log;

/// The base size of the rem: sizes in this file are in px at this rem, then follow the UI size.
const BASE_REM: f32 = 16.0;
/// The blurred edge of the soft bar.
const EDGE: f32 = 4.0;
/// The line a one-line row label sits in (the title line is `rems_from_px(16)`).
const LINE: f32 = 16.0;
/// Width of a glyph the font cannot give, as a part of the font size: wide (CJK, emoji).
const WIDE_EM: f32 = 1.0;
/// Fewest characters the box is wide for, so a short text does not leave a dot.
const MIN_EM: f32 = 1.6;

/// The width of a text from the advance of each character. A character the font has no glyph for
/// counts as wide when it is not ASCII.
fn text_width(text: &str, size: f32, advance: impl Fn(char) -> Option<f32>) -> f32 {
    let width: f32 = text
        .chars()
        .map(|ch| {
            advance(ch).unwrap_or(if ch.is_ascii() {
                size * 0.5
            } else {
                size * WIDE_EM
            })
        })
        .sum();
    width.max(size * MIN_EM)
}

/// The start of a text that is longer than a row can show.
fn capped(text: SharedString) -> SharedString {
    let whole: &str = text.as_ref();
    match whole.char_indices().nth(MAX_CHARS) {
        Some((end, _)) => SharedString::from(whole[..end].to_owned()),
        None => text,
    }
}

/// True while a blurred text still waits for its picture. The shots wait for it.
pub fn pictures_pending(cx: &App) -> bool {
    cx.try_global::<BlurCache>().is_some_and(BlurCache::pending)
}

/// A blurred text, in a box as wide as the text in the same font and size. It follows the UI
/// size, and shrinks with its row like a truncated text.
#[derive(IntoElement)]
pub struct BlurBar {
    text: SharedString,
    /// Font size in px at the base rem, as in `rems_from_px(..)` of the label it replaces.
    size: f32,
    weight: FontWeight,
    mono: bool,
    color: Color,
}

impl BlurBar {
    pub fn new(text: impl Into<SharedString>, size: f32) -> Self {
        Self {
            text: capped(text.into()),
            size,
            weight: FontWeight::NORMAL,
            mono: false,
            color: Color::Default,
        }
    }

    pub fn weight(mut self, weight: FontWeight) -> Self {
        self.weight = weight;
        self
    }

    /// For text the label draws in the buffer font (terminal tags).
    pub fn mono(mut self) -> Self {
        self.mono = true;
        self
    }

    /// The colour of the label it replaces.
    pub fn color(mut self, color: Color) -> Self {
        self.color = color;
        self
    }

    fn font(&self, window: &Window, cx: &App) -> Font {
        let mut font = if self.mono {
            theme::theme_settings(cx).buffer_font(cx).clone()
        } else {
            window.text_style().font()
        };
        font.weight = self.weight;
        font
    }

    fn measure(&self, font: &Font, size: f32, cx: &App) -> f32 {
        let system = cx.text_system();
        let font_id = system.resolve_font(font);
        text_width(&self.text, size, |ch| {
            system
                .advance(font_id, px(size), ch)
                .ok()
                .map(|advance| f32::from(advance.width))
        })
    }

    /// Measures a new text, keeps it as pending and renders its picture in the background.
    fn start(
        &self,
        key: BlurKey,
        font: &Font,
        size: f32,
        line: f32,
        window: &mut Window,
        cx: &mut App,
    ) -> Entry {
        let width = self.measure(font, size, cx);
        let dropped = cx.default_global::<BlurCache>().start(key.clone(), width);
        for image in dropped {
            cx.drop_image(image, Some(&mut *window));
        }
        let svg = key.svg(width, line);
        let renderer = cx.svg_renderer();
        // The first text an SVG draws reads the system's font list, which is slow.
        let render = cx.background_executor().spawn(async move {
            renderer
                .render_single_frame(svg.as_bytes(), 1.0)
                .map_err(|error| error.to_string())
        });
        cx.spawn(async move |cx| {
            let result = render.await;
            cx.update(|cx| {
                let error = result.as_ref().err().cloned();
                let first_failure = cx.default_global::<BlurCache>().finish(&key, result.ok());
                if let Some(error) = error.filter(|_| first_failure) {
                    app_log::write(&format!("blurred text: no picture: {error}"));
                }
                cx.refresh_windows();
            });
        })
        .detach();
        Entry {
            width,
            picture: Picture::Pending,
        }
    }
}

impl RenderOnce for BlurBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = f32::from(window.rem_size()) / BASE_REM;
        let size = self.size * scale;
        let line = LINE * scale;
        let room = ROOM * scale;
        let color = self.color.color(cx);
        let font = self.font(window, cx);
        let key = BlurKey::new(self.text.clone(), &font, self.mono, size, scale, color);
        let known = cx.default_global::<BlurCache>().get(&key);
        let entry = match known {
            Some(entry) => entry,
            None => self.start(key, &font, size, line, window, cx),
        };
        let width = entry.width;
        let inner = match entry.picture {
            Picture::Ready(image) => img(image)
                .absolute()
                .left_0()
                .top_0()
                .w(px(width + 2.0 * room))
                .h(px(line + 2.0 * room))
                .into_any_element(),
            Picture::Pending | Picture::Failed => {
                // A bar of the letters' height with a blurred glow of its own colour. A drop
                // shadow draws only outside its box, so the box is the bar.
                let height = (self.size * 0.6).round() * scale;
                let soft = color.opacity(0.3);
                div()
                    .absolute()
                    .left(px(room))
                    .top(px(room + (line - height) / 2.0))
                    .w(px(width))
                    .h(px(height))
                    .rounded_full()
                    .bg(soft)
                    .shadow(vec![BoxShadow {
                        color: soft,
                        offset: point(px(0.0), px(0.0)),
                        blur_radius: px(EDGE * scale),
                        spread_radius: px(0.0),
                        inset: false,
                    }])
                    .into_any_element()
            }
        };
        // The box is the plain text's box, so a blurred row keeps its layout. The blur reaches
        // past the box by `room`; a box that shrank with its row cuts the picture there.
        div().relative().min_w_0().w(px(width)).h(px(line)).child(
            div()
                .absolute()
                .top(px(-room))
                .bottom(px(-room))
                .left(px(-room))
                .right(px(-room))
                .overflow_hidden()
                .child(inner),
        )
    }
}

/// The one place a row decides what a private text shows: the bar for a blurred project, the plain
/// label otherwise. Each side is built only when it is used.
pub fn private_text(
    blurred: bool,
    bar: impl FnOnce() -> BlurBar,
    plain: impl FnOnce() -> AnyElement,
) -> AnyElement {
    if blurred {
        bar().into_any_element()
    } else {
        plain()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::Cell;

    use super::*;

    fn even(ch: char) -> Option<f32> {
        ch.is_ascii().then_some(7.0)
    }

    #[test]
    fn width_follows_the_text() {
        assert_eq!(text_width("abcdef", 13.0, even), 42.0);
        assert!(text_width("a longer title", 13.0, even) > text_width("short", 13.0, even));
    }

    #[test]
    fn wide_characters_count_as_wide() {
        assert_eq!(text_width("日本語日本語", 12.0, even), 72.0);
    }

    #[test]
    fn a_short_text_keeps_a_minimum_width() {
        assert_eq!(text_width("", 10.0, even), 16.0);
    }

    #[test]
    fn a_long_text_is_cut_at_a_character() {
        let long: String = "日".repeat(MAX_CHARS + 5);
        assert_eq!(capped(long.into()).chars().count(), MAX_CHARS);
        assert_eq!(capped("short".into()), SharedString::from("short"));
    }

    #[test]
    fn a_project_that_is_not_blurred_gets_the_plain_label() {
        let bars = Cell::new(0);
        let plains = Cell::new(0);
        let pick = |blurred| {
            private_text(
                blurred,
                || {
                    bars.set(bars.get() + 1);
                    BlurBar::new("secret", 13.0)
                },
                || {
                    plains.set(plains.get() + 1);
                    div().into_any_element()
                },
            )
        };
        pick(false);
        assert_eq!((bars.get(), plains.get()), (0, 1));
        pick(true);
        assert_eq!((bars.get(), plains.get()), (1, 1));
    }
}
