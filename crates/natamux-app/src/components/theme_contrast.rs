//! Colour maths, which theme colour each task state uses, and the contrast check of the Natamux
//! theme (docs/ui-spec.md section 6 rule 1). Works on the theme's JSON, so it needs no gpui.

// The colour maths serve the contrast tests; the app reads only `state_status`.
#![cfg_attr(not(test), allow(dead_code))]

use natamux_proto::tasks::task::TaskState;
use serde_json::Value;

/// A task state shows in one of Zed's status colours of the theme: text and glyph, fill, border.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatusKey {
    Ignored,
    Hint,
    Info,
    Warning,
    Predictive,
    Conflict,
    Error,
    Success,
    Hidden,
}

impl StatusKey {
    pub const ALL: [StatusKey; 9] = [
        StatusKey::Ignored,
        StatusKey::Hint,
        StatusKey::Info,
        StatusKey::Warning,
        StatusKey::Predictive,
        StatusKey::Conflict,
        StatusKey::Error,
        StatusKey::Success,
        StatusKey::Hidden,
    ];

    /// The theme JSON key of the text and glyph colour. `<key>.background` is the fill.
    pub fn json(self) -> &'static str {
        match self {
            StatusKey::Ignored => "ignored",
            StatusKey::Hint => "hint",
            StatusKey::Info => "info",
            StatusKey::Warning => "warning",
            StatusKey::Predictive => "predictive",
            StatusKey::Conflict => "conflict",
            StatusKey::Error => "error",
            StatusKey::Success => "success",
            StatusKey::Hidden => "hidden",
        }
    }
}

/// The colour of a state. "Needs you" (signal) is the same colour as needs-input.
pub fn state_status(state: TaskState) -> StatusKey {
    match state {
        TaskState::Draft => StatusKey::Ignored,
        TaskState::Ready => StatusKey::Hint,
        TaskState::Running => StatusKey::Info,
        TaskState::NeedsInput => StatusKey::Warning,
        TaskState::Review => StatusKey::Predictive,
        TaskState::Blocked => StatusKey::Conflict,
        TaskState::Failed => StatusKey::Error,
        TaskState::Done => StatusKey::Success,
        TaskState::Cancelled | TaskState::Unknown => StatusKey::Hidden,
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

/// `#rrggbb` or `#rrggbbaa`.
pub fn parse_hex(text: &str) -> Option<Rgba> {
    let digits = text.strip_prefix('#')?;
    if !(digits.len() == 6 || digits.len() == 8) || !digits.is_ascii() {
        return None;
    }
    let byte = |at: usize| u8::from_str_radix(digits.get(at..at + 2)?, 16).ok();
    let a = if digits.len() == 8 {
        f64::from(byte(6)?) / 255.0
    } else {
        1.0
    };
    Some(Rgba {
        r: byte(0)?,
        g: byte(2)?,
        b: byte(4)?,
        a,
    })
}

/// `top` laid over an opaque `base`.
pub fn blend(top: Rgba, base: Rgba) -> Rgba {
    let mix = |t: u8, b: u8| (f64::from(t) * top.a + f64::from(b) * (1.0 - top.a)).round() as u8;
    Rgba {
        r: mix(top.r, base.r),
        g: mix(top.g, base.g),
        b: mix(top.b, base.b),
        a: 1.0,
    }
}

fn luminance(c: Rgba) -> f64 {
    let lin = |v: u8| {
        let v = f64::from(v) / 255.0;
        if v <= 0.03928 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    0.2126 * lin(c.r) + 0.7152 * lin(c.g) + 0.0722 * lin(c.b)
}

/// WCAG 2.x contrast ratio. A see-through `fg` is first laid over `bg`.
pub fn ratio(fg: Rgba, bg: Rgba) -> f64 {
    let fg = if fg.a < 1.0 { blend(fg, bg) } else { fg };
    let (a, b) = (luminance(fg), luminance(bg));
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

/// A colour of a theme style: a key such as `text.muted` or `accents.2`, or `top>base` for a
/// see-through colour over what is behind it.
fn colour(style: &Value, expr: &str) -> Option<Rgba> {
    if let Some((top, base)) = expr.split_once('>') {
        return Some(blend(colour(style, top)?, colour(style, base)?));
    }
    let value = match expr.strip_prefix("accents.") {
        Some(at) => style.get("accents")?.get(at.parse::<usize>().ok()?)?,
        None => match expr.strip_prefix("syntax.") {
            Some(name) => style.get("syntax")?.get(name)?.get("color")?,
            None => style.get(expr)?,
        },
    };
    parse_hex(value.as_str()?)
}

const TEXT: f64 = 4.5;
const GLYPH: f64 = 3.0;

/// Colours that text, icons and focus rings sit on.
const SURFACES: [&str; 8] = [
    "background",
    "surface.background",
    "editor.background",
    "elevated_surface.background",
    "element.background",
    "title_bar.background",
    "status_bar.background",
    "panel.background",
];

/// Selected and hovered rows, over the surface they are on.
const ROWS: [&str; 5] = [
    "element.selection_background>editor.background",
    "ghost_element.hover>background",
    "ghost_element.active>background",
    "ghost_element.selected>background",
    "ghost_element.hover>editor.background",
];

const ANSI: [&str; 7] = ["red", "green", "yellow", "blue", "magenta", "cyan", "white"];

/// (name, text colour, background, least ratio) of every pair the theme must keep.
fn pairs(style: &Value) -> Vec<(String, String, String, f64)> {
    let mut out: Vec<(String, String, String, f64)> = Vec::new();
    let mut add = |fg: &str, bg: &str, min: f64| {
        out.push((format!("{fg} on {bg}"), fg.to_string(), bg.to_string(), min));
    };
    let editor = "editor.background";
    for bg in SURFACES.into_iter().chain(ROWS) {
        add("text", bg, TEXT);
        add("text.muted", bg, TEXT);
        add("icon", bg, GLYPH);
        add("icon.muted", bg, GLYPH);
        add("border.focused", bg, GLYPH);
    }
    // A placeholder sits in a field, on a surface or the editor colour, never on the shell.
    for bg in ["surface.background", editor] {
        add("text.placeholder", bg, TEXT);
    }
    for bg in ["background", "surface.background", editor] {
        add("text.accent", bg, TEXT);
        add("link_text.hover", bg, TEXT);
        add("icon.accent", bg, GLYPH);
        add("panel.focused_border", bg, GLYPH);
        add("pane.focused_border", bg, GLYPH);
    }
    for key in StatusKey::ALL {
        let fg = key.json();
        let own = format!("{fg}.background>background");
        add(fg, &own, TEXT);
        for bg in ["background", "surface.background", editor] {
            add(fg, bg, TEXT);
        }
    }
    for fg in ["created", "deleted", "modified", "renamed"] {
        add(fg, &format!("{fg}.background>{editor}"), TEXT);
        add(fg, editor, TEXT);
    }
    for at in 0..6 {
        for bg in ["background", "surface.background", editor] {
            add(&format!("accents.{at}"), bg, TEXT);
        }
    }
    if let Some(syntax) = style.get("syntax").and_then(Value::as_object) {
        for name in syntax.keys() {
            add(&format!("syntax.{name}"), editor, TEXT);
        }
    }
    let terminal = "terminal.background";
    add("terminal.foreground", terminal, TEXT);
    add("terminal.bright_foreground", terminal, TEXT);
    // Black is a background colour in practice; the terminal lifts it when it is used as text.
    for name in ANSI {
        add(&format!("terminal.ansi.{name}"), terminal, TEXT);
        add(&format!("terminal.ansi.bright_{name}"), terminal, TEXT);
    }
    add("terminal.ansi.bright_black", terminal, TEXT);
    out
}

/// One line for every pair that is missing or too weak. Empty: the style passes.
pub fn check_style(style: &Value) -> Vec<String> {
    let mut failures = Vec::new();
    for (name, fg, bg, min) in pairs(style) {
        match (colour(style, &fg), colour(style, &bg)) {
            (Some(fg), Some(bg)) => {
                let got = ratio(fg, bg);
                if got < min {
                    failures.push(format!("{name}: {got:.2}:1, needs {min}:1"));
                }
            }
            _ => failures.push(format!("{name}: a colour is missing or not #rrggbb[aa]")),
        }
    }
    failures
}

/// The style of one theme of a family file.
pub fn theme_style<'a>(family: &'a Value, name: &str) -> Option<&'a Value> {
    family
        .get("themes")?
        .as_array()?
        .iter()
        .find(|theme| theme.get("name").and_then(Value::as_str) == Some(name))?
        .get("style")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const FAMILY: &str = include_str!("../../assets/themes/natamux.json");
    const THEMES: [&str; 2] = ["Natamux Light", "Natamux Dark"];

    fn family() -> Value {
        serde_json::from_str(FAMILY).expect("theme file is JSON")
    }

    #[test]
    fn natamux_themes_pass_contrast() {
        let family = family();
        for name in THEMES {
            let style = theme_style(&family, name).expect("theme exists");
            let failures = check_style(style);
            assert!(failures.is_empty(), "{name}:\n{}", failures.join("\n"));
        }
    }

    #[test]
    fn a_text_pair_under_four_and_a_half_fails() {
        let family = family();
        for name in THEMES {
            let mut style = theme_style(&family, name).expect("theme exists").clone();
            // Mid grey is about 4:1 on both the light and the dark surfaces.
            style["text"] = json!("#777777ff");
            let failures = check_style(&style);
            assert!(
                failures
                    .iter()
                    .any(|line| line.starts_with("text on background")),
                "{name}: {failures:?}"
            );
        }
    }

    #[test]
    fn a_weak_focus_ring_and_a_missing_colour_fail() {
        let family = family();
        let mut style = theme_style(&family, "Natamux Dark").expect("theme").clone();
        style["border.focused"] = json!("#16171a");
        style.as_object_mut().expect("object").remove("icon.muted");
        let failures = check_style(&style);
        assert!(
            failures
                .iter()
                .any(|line| line.starts_with("border.focused on"))
        );
        assert!(
            failures
                .iter()
                .any(|line| line.starts_with("icon.muted on"))
        );
    }

    #[test]
    fn ratio_matches_the_wcag_values() {
        let black = parse_hex("#000000").expect("hex");
        let white = parse_hex("#ffffff").expect("hex");
        assert!((ratio(black, white) - 21.0).abs() < 0.01);
        assert!((ratio(white, white) - 1.0).abs() < 0.001);
        // docs/ui-spec.md 2.3: ink on the dark shell is 15.7.
        let ink = parse_hex("#e6e7ea").expect("hex");
        let shell = parse_hex("#0c0d0f").expect("hex");
        assert!((ratio(ink, shell) - 15.7).abs() < 0.1);
    }

    #[test]
    fn hex_parsing_takes_six_or_eight_digits_only() {
        assert_eq!(
            parse_hex("#7aa2ff").map(|c| (c.r, c.g, c.b)),
            Some((122, 162, 255))
        );
        assert!(parse_hex("#7aa2ff80").is_some_and(|c| (c.a - 0.502).abs() < 0.01));
        for bad in ["7aa2ff", "#7aa2f", "#7aa2ffg0", "#é1234", ""] {
            assert!(parse_hex(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn a_see_through_colour_is_laid_over_its_base() {
        let half_white = parse_hex("#ffffff80").expect("hex");
        let black = parse_hex("#000000").expect("hex");
        let mixed = blend(half_white, black);
        assert_eq!((mixed.r, mixed.g, mixed.b), (128, 128, 128));
    }

    #[test]
    fn every_state_has_a_colour_and_needs_input_is_the_signal() {
        let states = [
            TaskState::Draft,
            TaskState::Ready,
            TaskState::Running,
            TaskState::NeedsInput,
            TaskState::Review,
            TaskState::Blocked,
            TaskState::Failed,
            TaskState::Done,
            TaskState::Cancelled,
        ];
        let mut seen = Vec::new();
        for state in states {
            let key = state_status(state);
            assert!(!seen.contains(&key), "{state:?} shares a colour");
            seen.push(key);
        }
        assert_eq!(state_status(TaskState::NeedsInput), StatusKey::Warning);
        assert_eq!(state_status(TaskState::Unknown), StatusKey::Hidden);
    }
}
