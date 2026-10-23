//! The terminal font: what is saved, what Ghostty gives, what a terminal uses, and the rules of
//! the Terminal font dialog (`features/terminals/terminal-font.ts`, `TerminalFontDialog.tsx`).

use serde_json::{Map, Value, json};

pub const MIN_SIZE: f32 = 8.0;
pub const MAX_SIZE: f32 = 32.0;
pub const BUILT_IN_FAMILY: &str = "JetBrains Mono";
pub const BUILT_IN_SIZE: f32 = 13.0;
pub const SIZE_REASON: &str = "Pick a size from 8 to 32";
pub const PREVIEW: &str = "git commit -m \"Fix café → naïve\" ✓ │ 0O 1lI {}";

/// A saved font. A part left out comes from Ghostty's or the built-in font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalFont {
    pub family: Option<String>,
    pub size: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSource {
    Custom,
    Ghostty,
    Default,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ResolvedFont {
    pub family: String,
    pub size: f32,
    pub source: FontSource,
}

fn in_range(size: f32) -> bool {
    size.is_finite() && (MIN_SIZE..=MAX_SIZE).contains(&size)
}

/// A font from storage, or None when it is not a font or has no valid part.
pub fn parse_terminal_font(json: &Value) -> Option<TerminalFont> {
    let object = json.as_object()?;
    let family = object
        .get("family")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_owned);
    let size = object
        .get("size")
        .and_then(Value::as_f64)
        .map(|size| size as f32)
        .filter(|size| in_range(*size));
    (family.is_some() || size.is_some()).then_some(TerminalFont { family, size })
}

/// The same from text; None for text that is not JSON.
// view.json keeps the font as JSON; the text form of the web's localStorage is kept with its test.
#[cfg_attr(not(test), allow(dead_code))]
pub fn parse_terminal_font_text(raw: &str) -> Option<TerminalFont> {
    parse_terminal_font(&serde_json::from_str(raw).ok()?)
}

impl TerminalFont {
    pub fn to_json(&self) -> Value {
        let mut object = Map::new();
        if let Some(family) = &self.family {
            object.insert("family".into(), json!(family));
        }
        if let Some(size) = self.size {
            object.insert("size".into(), json!(f64_of(size)));
        }
        Value::Object(object)
    }
}

// The shortest text of the f32, so 13.25 stays 13.25.
fn f64_of(size: f32) -> f64 {
    size.to_string().parse().unwrap_or(f64::from(size))
}

/// A quote or a backslash in a family would break a font list: they are left out.
pub fn clean_family(family: &str) -> String {
    family.replace(['"', '\\'], "").trim().to_owned()
}

/// Each part from the user's font first, then Ghostty's, then the built-in one.
pub fn terminal_font(
    custom: Option<&TerminalFont>,
    ghostty: Option<&TerminalFont>,
) -> ResolvedFont {
    let family = custom
        .and_then(|font| font.family.as_deref())
        .or_else(|| ghostty.and_then(|font| font.family.as_deref()))
        .map(clean_family)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| BUILT_IN_FAMILY.to_owned());
    let size = custom
        .and_then(|font| font.size)
        .or_else(|| ghostty.and_then(|font| font.size))
        .unwrap_or(BUILT_IN_SIZE);
    let source = if custom.is_some() {
        FontSource::Custom
    } else if ghostty.is_some() {
        FontSource::Ghostty
    } else {
        FontSource::Default
    };
    ResolvedFont {
        family,
        size,
        source,
    }
}

/// What an empty family field uses: shown as its placeholder.
pub fn fallback_family(ghostty: Option<&TerminalFont>) -> String {
    ghostty
        .and_then(|font| font.family.clone())
        .unwrap_or_else(|| BUILT_IN_FAMILY.to_owned())
}

/// What an empty size field uses: shown as its placeholder.
pub fn fallback_size(ghostty: Option<&TerminalFont>) -> f32 {
    ghostty.and_then(|font| font.size).unwrap_or(BUILT_IN_SIZE)
}

/// The size of the field: the fallback when it is empty. None unless a number from 8 to 32.
pub fn size_ok(field: &str, fallback: f32) -> Option<f32> {
    let text = field.trim();
    let size = if text.is_empty() {
        fallback
    } else {
        text.parse::<f32>().ok()?
    };
    in_range(size).then_some(size)
}

/// The font Save stores: None when both fields are empty. Check `size_ok` first.
pub fn font_to_save(family_field: &str, size_field: &str) -> Option<TerminalFont> {
    let family = family_field.trim();
    let font = TerminalFont {
        family: (!family.is_empty()).then(|| family.to_owned()),
        size: size_field
            .trim()
            .parse::<f32>()
            .ok()
            .filter(|s| in_range(*s)),
    };
    (font.family.is_some() || font.size.is_some()).then_some(font)
}

pub fn dialog_description(ghostty: Option<&TerminalFont>) -> String {
    let family = fallback_family(ghostty);
    let size = fallback_size(ghostty);
    if ghostty.is_some() {
        format!("Empty fields use your Ghostty font: {family}, {size}px.")
    } else {
        format!(
            "Empty fields use the built-in font, {family}, {size}px. A font that is not installed falls back to it."
        )
    }
}

/// The ghost button that clears a custom font.
pub fn reset_label(ghostty_present: bool) -> &'static str {
    if ghostty_present {
        "Use Ghostty font"
    } else {
        "Use built-in font"
    }
}

#[cfg(test)]
#[path = "terminal_font_tests.rs"]
mod tests;
