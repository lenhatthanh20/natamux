//! The picture of a blurred text (docs/ui-spec.md, "Blurred project"): an SVG with the text and a
//! Gaussian blur, which GPUI's SVG renderer turns into an image. This file builds the SVG and
//! keeps the pictures; `blur_bar.rs` draws them.

use std::collections::HashMap;
use std::sync::Arc;

use gpui::{Font, Global, Hsla, RenderImage, Rgba, SharedString};

/// The blur's standard deviation in px at the base rem, as the web's `filter: blur(5px)`.
const BLUR: f32 = 5.0;
/// Room around the text for the blur, in px at the base rem, so the picture's edge does not cut it.
pub(super) const ROOM: f32 = 10.0;
/// A full cache is cleared. A picture of a title is near 100 KB, so this keeps the cache small.
const CACHE_LIMIT: usize = 512;
/// shortcut: a longer text is cut before it is measured and drawn, since no sidebar row shows
/// more; raise it when a blurred text ends before its row does.
pub(super) const MAX_CHARS: usize = 160;

/// What gives a picture: the text, its font and colour, and the UI scale.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct BlurKey {
    text: SharedString,
    family: SharedString,
    weight: u32,
    mono: bool,
    size: u32,
    scale: u32,
    color: [u8; 4],
}

impl BlurKey {
    /// `size`: the font size in px on screen.
    pub(super) fn new(
        text: SharedString,
        font: &Font,
        mono: bool,
        size: f32,
        scale: f32,
        color: Hsla,
    ) -> Self {
        Self {
            text,
            family: font.family.clone(),
            weight: font.weight.0.to_bits(),
            mono,
            size: size.to_bits(),
            scale: scale.to_bits(),
            color: color_bytes(color),
        }
    }

    /// The SVG of the key's text, in a box of `width` x `line` px with `ROOM` on each side.
    pub(super) fn svg(&self, width: f32, line: f32) -> String {
        blur_svg(&SvgText {
            text: &self.text,
            family: &self.family,
            mono: self.mono,
            weight: f32::from_bits(self.weight),
            size: f32::from_bits(self.size),
            scale: f32::from_bits(self.scale),
            color: self.color,
            width,
            line,
        })
    }
}

fn color_bytes(color: Hsla) -> [u8; 4] {
    let color = Rgba::from(color);
    [color.r, color.g, color.b, color.a].map(|part| (part.clamp(0.0, 1.0) * 255.0).round() as u8)
}

#[derive(Clone)]
pub(super) enum Picture {
    /// The render runs in the background.
    Pending,
    Ready(Arc<RenderImage>),
    Failed,
}

#[derive(Clone)]
pub(super) struct Entry {
    /// The layout width of the text in px.
    pub width: f32,
    pub picture: Picture,
}

#[derive(Default)]
pub(super) struct BlurCache {
    entries: HashMap<BlurKey, Entry>,
    failure_logged: bool,
}

impl Global for BlurCache {}

impl BlurCache {
    pub(super) fn get(&self, key: &BlurKey) -> Option<Entry> {
        self.entries.get(key).cloned()
    }

    /// Keeps a new text as pending. A full cache is cleared first: the pictures it held are
    /// returned, so the caller can free their textures.
    pub(super) fn start(&mut self, key: BlurKey, width: f32) -> Vec<Arc<RenderImage>> {
        let mut dropped = Vec::new();
        if self.entries.len() >= CACHE_LIMIT {
            for (_, entry) in self.entries.drain() {
                if let Picture::Ready(image) = entry.picture {
                    dropped.push(image);
                }
            }
        }
        self.entries.insert(
            key,
            Entry {
                width,
                picture: Picture::Pending,
            },
        );
        dropped
    }

    /// Stores the result of a render. A key the cache dropped meanwhile stays dropped. True for
    /// the first failure, which the caller logs.
    pub(super) fn finish(&mut self, key: &BlurKey, image: Option<Arc<RenderImage>>) -> bool {
        let failed = image.is_none();
        if let Some(entry) = self.entries.get_mut(key) {
            entry.picture = image.map_or(Picture::Failed, Picture::Ready);
        }
        let first = failed && !self.failure_logged;
        self.failure_logged |= failed;
        first
    }

    /// True while a picture is still rendering.
    pub(super) fn pending(&self) -> bool {
        self.entries
            .values()
            .any(|entry| matches!(entry.picture, Picture::Pending))
    }
}

/// What the SVG of a blurred text is made from. Sizes are in px on screen.
struct SvgText<'a> {
    text: &'a str,
    family: &'a str,
    mono: bool,
    weight: f32,
    size: f32,
    /// The UI scale: the blur and the room follow it.
    scale: f32,
    /// Red, green, blue, alpha.
    color: [u8; 4],
    /// The box of the plain text.
    width: f32,
    line: f32,
}

/// The font family for the SVG renderer, which does not know GPUI's names for the bundled fonts.
fn svg_family(family: &str) -> &str {
    match family {
        ".ZedSans" => "IBM Plex Sans",
        ".ZedMono" => "Lilex",
        other => other,
    }
}

/// Text that is safe inside an SVG element or attribute. A title must not add elements: the
/// renderer can load image files. A control character becomes a space, since XML has none.
fn escape(text: &str) -> String {
    let mut safe = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => safe.push_str("&amp;"),
            '<' => safe.push_str("&lt;"),
            '>' => safe.push_str("&gt;"),
            '"' => safe.push_str("&quot;"),
            '\'' => safe.push_str("&apos;"),
            ch if ch.is_control() => safe.push(' '),
            ch => safe.push(ch),
        }
    }
    safe
}

fn blur_svg(text: &SvgText) -> String {
    let room = ROOM * text.scale;
    let width = text.width + 2.0 * room;
    let height = text.line + 2.0 * room;
    // The middle of the letters sits on the middle of the line, near where the label draws them.
    let baseline = room + text.line / 2.0 + text.size * 0.35;
    let [red, green, blue, alpha] = text.color;
    let generic = if text.mono { "monospace" } else { "sans-serif" };
    format!(
        concat!(
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{width:.2}" height="{height:.2}">"#,
            // The filter covers the whole picture; the default region would cut the blur. The
            // web's blur works in sRGB.
            r#"<filter id="b" filterUnits="userSpaceOnUse" x="0" y="0" width="{width:.2}" "#,
            r#"height="{height:.2}" color-interpolation-filters="sRGB">"#,
            r#"<feGaussianBlur stdDeviation="{blur:.2}"/></filter>"#,
            r#"<text x="{room:.2}" y="{baseline:.2}" font-family="'{family}', {generic}" "#,
            r#"font-size="{size:.2}" font-weight="{weight}" "#,
            r##"fill="#{red:02x}{green:02x}{blue:02x}" fill-opacity="{opacity:.3}" "##,
            r#"filter="url(#b)" xml:space="preserve">{body}</text></svg>"#,
        ),
        width = width,
        height = height,
        blur = BLUR * text.scale,
        room = room,
        baseline = baseline,
        family = escape(svg_family(text.family)),
        generic = generic,
        size = text.size,
        weight = text.weight.round() as u32,
        red = red,
        green = green,
        blue = blue,
        opacity = f32::from(alpha) / 255.0,
        body = escape(text.text),
    )
}

#[cfg(test)]
mod tests {
    use gpui::{FontWeight, SvgRenderer, black, font, red};

    use super::*;
    use crate::app::assets::NatamuxAssets;

    fn sample(text: &str) -> SvgText<'_> {
        SvgText {
            text,
            family: ".ZedSans",
            mono: false,
            weight: 600.0,
            size: 13.0,
            scale: 1.0,
            color: [0x12, 0xab, 0xff, 0x80],
            width: 60.0,
            line: 16.0,
        }
    }

    fn key(text: &'static str, size: f32, color: Hsla) -> BlurKey {
        let mut font = font(".ZedSans");
        font.weight = FontWeight::NORMAL;
        BlurKey::new(text.into(), &font, false, size, 1.0, color)
    }

    #[test]
    fn a_title_cannot_add_svg_elements() {
        let svg = blur_svg(&sample(r#"</text><image href="x"/> & 'more'"#));
        assert!(!svg.contains("<image"));
        assert_eq!(svg.matches("<text").count(), 1);
        assert_eq!(svg.matches("</text>").count(), 1);
        assert!(
            svg.contains("&lt;/text&gt;&lt;image href=&quot;x&quot;/&gt; &amp; &apos;more&apos;")
        );
    }

    #[test]
    fn a_control_character_becomes_a_space() {
        assert_eq!(escape("a\u{0}b\tc"), "a b c");
    }

    #[test]
    fn an_entity_in_a_title_is_escaped_once() {
        assert_eq!(escape("&lt;"), "&amp;lt;");
    }

    #[test]
    fn an_empty_and_a_long_text_give_one_text_element() {
        let long = "a<b ".repeat(MAX_CHARS);
        for text in ["", long.as_str()] {
            let svg = blur_svg(&sample(text));
            assert_eq!(svg.matches('<').count(), 7);
            assert!(svg.ends_with("</text></svg>"));
        }
    }

    #[test]
    fn a_font_family_cannot_leave_its_attribute() {
        let mut text = sample("title");
        text.family = r#"x" onload="y"#;
        let svg = blur_svg(&text);
        assert!(svg.contains(r#"font-family="'x&quot; onload=&quot;y', sans-serif""#));
    }

    #[test]
    fn the_svg_carries_the_font_the_colour_and_the_blur() {
        let svg = blur_svg(&sample("title"));
        assert!(svg.contains(r#"font-family="'IBM Plex Sans', sans-serif""#));
        assert!(svg.contains(r#"font-size="13.00""#));
        assert!(svg.contains(r#"font-weight="600""#));
        // The fill is the colour as hex, with the alpha beside it.
        assert!(svg.contains(r#"12abff" fill-opacity="0.502""#));
        assert!(svg.contains(r#"stdDeviation="5.00""#));
        // The text box and the room around it.
        assert!(svg.contains(r#"width="80.00" height="36.00">"#));
        assert!(svg.contains(r#"<text x="10.00""#));
    }

    #[test]
    fn the_blur_and_the_room_follow_the_ui_size() {
        let mut text = sample("title");
        text.scale = 1.5;
        let svg = blur_svg(&text);
        assert!(svg.contains(r#"stdDeviation="7.50""#));
        assert!(svg.contains(r#"<text x="15.00""#));
    }

    #[test]
    fn a_mono_text_falls_back_to_a_mono_font() {
        let mut text = sample("T-0001");
        text.family = ".ZedMono";
        text.mono = true;
        assert!(blur_svg(&text).contains(r#"font-family="'Lilex', monospace""#));
    }

    #[test]
    fn a_text_is_rendered_once_per_font_size_and_colour() {
        let ink = black();
        let muted = red();
        let mut cache = BlurCache::default();
        assert!(cache.get(&key("title", 13.0, ink)).is_none());
        cache.start(key("title", 13.0, ink), 50.0);
        // The second draw finds the entry, so it starts no render.
        let known = cache.get(&key("title", 13.0, ink));
        assert!(matches!(
            known,
            Some(Entry {
                picture: Picture::Pending,
                ..
            })
        ));
        assert_eq!(known.map(|entry| entry.width), Some(50.0));
        assert!(cache.pending());
        assert!(cache.get(&key("title", 14.0, ink)).is_none());
        assert!(cache.get(&key("title", 13.0, muted)).is_none());
        assert!(cache.get(&key("other", 13.0, ink)).is_none());
    }

    #[test]
    fn a_failed_render_is_kept_and_reported_once() {
        let ink = black();
        let mut cache = BlurCache::default();
        cache.start(key("one", 13.0, ink), 20.0);
        cache.start(key("two", 13.0, ink), 20.0);
        assert!(cache.finish(&key("one", 13.0, ink), None));
        assert!(!cache.finish(&key("two", 13.0, ink), None));
        assert!(!cache.pending());
        assert!(matches!(
            cache.get(&key("one", 13.0, ink)),
            Some(Entry {
                picture: Picture::Failed,
                ..
            })
        ));
    }

    #[test]
    fn a_result_for_a_dropped_text_is_not_kept() {
        let ink = black();
        let mut cache = BlurCache::default();
        cache.finish(&key("gone", 13.0, ink), None);
        assert!(cache.get(&key("gone", 13.0, ink)).is_none());
    }

    #[test]
    fn a_full_cache_is_cleared() {
        let ink = black();
        let mut cache = BlurCache::default();
        for size in 0..CACHE_LIMIT {
            cache.start(key("title", size as f32, ink), 20.0);
        }
        assert_eq!(cache.entries.len(), CACHE_LIMIT);
        cache.start(key("next", 13.0, ink), 20.0);
        assert_eq!(cache.entries.len(), 1);
    }

    #[test]
    fn the_picture_of_a_text_is_blurred() {
        let renderer = SvgRenderer::new(Arc::new(NatamuxAssets));
        let mut text = sample("abc");
        text.color = [0, 0, 0, 255];
        let image = renderer
            .render_single_frame(blur_svg(&text).as_bytes(), 1.0)
            .expect("the svg renders");
        let size = image.size(0);
        assert!(i32::from(size.width) > 0 && i32::from(size.height) > 0);
        let bytes = image.as_bytes(0).expect("one frame");
        let (pixels, _) = bytes.as_chunks::<4>();
        let alphas: Vec<u8> = pixels.iter().map(|pixel| pixel[3]).collect();
        // Sharp letters have solid pixels; a blur of 5 px leaves none.
        assert!(alphas.iter().any(|alpha| (1..255).contains(alpha)));
        assert!(alphas.iter().all(|alpha| *alpha < 255));
    }
}
