//! The terminal font: what is saved, what a terminal uses, and the rules of the Terminal font
//! dialog (`features/terminals/terminal-font.ts`, `TerminalFontDialog.tsx`). Unlike the web, the
//! app reads no Ghostty config: with no font picked, a terminal is as Zed's (owner, 2026-10-24).

use serde_json::{Map, Value, json};

pub const MIN_SIZE: f32 = 8.0;
pub const MAX_SIZE: f32 = 32.0;
/// Zed's buffer font (`.ZedMono`) and size: a terminal with no font of its own uses them, as Zed.
pub const BUILT_IN_FAMILY: &str = "Lilex";
pub const BUILT_IN_SIZE: f32 = 13.0;
pub const SIZE_REASON: &str = "Pick a size from 8 to 32";
pub const PREVIEW: &str = "git commit -m \"Fix café → naïve\" ✓ │ 0O 1lI {}";

/// A saved font. A part left out comes from the built-in font.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct TerminalFont {
    pub family: Option<String>,
    pub size: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontSource {
    Custom,
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

/// Each part from the user's font, else the built-in one.
pub fn terminal_font(custom: Option<&TerminalFont>) -> ResolvedFont {
    let family = custom
        .and_then(|font| font.family.as_deref())
        .map(clean_family)
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| BUILT_IN_FAMILY.to_owned());
    let size = custom.and_then(|font| font.size).unwrap_or(BUILT_IN_SIZE);
    let source = if custom.is_some() {
        FontSource::Custom
    } else {
        FontSource::Default
    };
    ResolvedFont {
        family,
        size,
        source,
    }
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

pub fn dialog_description() -> String {
    format!(
        "Empty fields use the built-in font, {BUILT_IN_FAMILY}, {BUILT_IN_SIZE}px. A font that is not installed falls back to it."
    )
}

/// The ghost button that clears a custom font.
pub const RESET_LABEL: &str = "Use built-in font";

#[cfg(test)]
#[path = "terminal_font_tests.rs"]
mod tests;
