//! What a blurred project shows in place of its private text (docs/ui-spec.md, "Blurred project").
//! GPUI has no text blur, so the row draws a soft bar as wide as the text would be: a drop shadow
//! with a blurred edge. The real text is never drawn.

use std::collections::HashMap;

use gpui::{AnyElement, BoxShadow, FontWeight, Global, point};
use ui::prelude::*;

/// The base size of the rem: sizes in this file are in px at this rem, then follow the UI size.
const BASE_REM: f32 = 16.0;
/// Room around the bar for its blurred edge, so the row's clip does not cut it.
const EDGE: f32 = 4.0;
/// The line a one-line row label sits in (the title line is `rems_from_px(16)`).
const LINE: f32 = 16.0;
/// Width of a glyph the font cannot give, as a part of the font size: wide (CJK, emoji).
const WIDE_EM: f32 = 1.0;
/// Fewest characters the bar is wide for, so a short text does not leave a dot.
const MIN_EM: f32 = 1.6;
const CACHE_LIMIT: usize = 4096;

/// The text, the font weight and kind, and the size in px that gave a width.
type WidthKey = (SharedString, u32, bool, u32);

#[derive(Default)]
struct WidthCache(HashMap<WidthKey, f32>);

impl Global for WidthCache {}

impl WidthCache {
    /// The cached width, or the measure's result, which is then kept.
    fn get_or_measure(&mut self, key: WidthKey, measure: impl FnOnce() -> f32) -> f32 {
        if let Some(width) = self.0.get(&key) {
            return *width;
        }
        let width = measure();
        if self.0.len() >= CACHE_LIMIT {
            self.0.clear();
        }
        self.0.insert(key, width);
        width
    }
}

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

/// A blurred bar for a text, as wide as the text in the same font and size. It follows the UI
/// size, and shrinks with its row like a truncated text.
#[derive(IntoElement)]
pub struct BlurBar {
    text: SharedString,
    /// Font size in px at the base rem, as in `rems_from_px(..)` of the label it replaces.
    size: f32,
    weight: FontWeight,
    mono: bool,
}

impl BlurBar {
    pub fn new(text: impl Into<SharedString>, size: f32) -> Self {
        Self {
            text: text.into(),
            size,
            weight: FontWeight::NORMAL,
            mono: false,
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

    fn measure(&self, size: Pixels, window: &Window, cx: &App) -> f32 {
        let mut font = if self.mono {
            theme::theme_settings(cx).buffer_font(cx).clone()
        } else {
            window.text_style().font()
        };
        font.weight = self.weight;
        let system = cx.text_system();
        let font_id = system.resolve_font(&font);
        let size = f32::from(size);
        text_width(&self.text, size, |ch| {
            system
                .advance(font_id, px(size), ch)
                .ok()
                .map(|advance| f32::from(advance.width))
        })
    }
}

impl RenderOnce for BlurBar {
    fn render(self, window: &mut Window, cx: &mut App) -> impl IntoElement {
        let scale = f32::from(window.rem_size()) / BASE_REM;
        let size = px(self.size * scale);
        let key = (
            self.text.clone(),
            self.weight.0.to_bits(),
            self.mono,
            f32::from(size).to_bits(),
        );
        let cached = cx.default_global::<WidthCache>().0.get(&key).copied();
        let width = match cached {
            Some(width) => width,
            None => {
                let width = self.measure(size, window, cx);
                cx.default_global::<WidthCache>()
                    .get_or_measure(key, || width)
            }
        };
        let edge = EDGE * scale;
        let line = LINE * scale;
        let height = (self.size * 0.6).round() * scale;
        // A bar of its real height with a blurred glow of its own colour: the glow softens the edge
        // and the ends are round. A drop shadow draws only outside its box, so the box is the bar.
        let color = cx.theme().colors().text.opacity(0.3);
        let core = div()
            .absolute()
            .left(px(edge))
            .top(px((line - height) / 2.0))
            .w(px(width))
            .h(px(height))
            .rounded_full()
            .bg(color)
            .shadow(vec![BoxShadow {
                color,
                offset: point(px(0.0), px(0.0)),
                blur_radius: px(edge),
                spread_radius: px(0.0),
                inset: false,
            }]);
        div()
            .relative()
            .flex_shrink()
            .min_w_0()
            .overflow_hidden()
            .w(px(width + 2.0 * edge))
            .h(px(line))
            .child(core)
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
    fn the_width_is_measured_once_per_text_and_size() {
        let mut cache = WidthCache::default();
        let calls = Cell::new(0);
        let key = |size: f32| (SharedString::from("title"), 0, false, size.to_bits());
        for _ in 0..3 {
            let width = cache.get_or_measure(key(13.0), || {
                calls.set(calls.get() + 1);
                50.0
            });
            assert_eq!(width, 50.0);
        }
        assert_eq!(calls.get(), 1);
        cache.get_or_measure(key(14.0), || {
            calls.set(calls.get() + 1);
            55.0
        });
        assert_eq!(calls.get(), 2);
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
