//! The theme value golden (docs/plan-mit-license.md 6.8, 7.6, S9.1): every value of the 17 themes
//! the app ships, as Zed's theme crate resolves them under the app's own settings. A theme file
//! sets only some keys; Zed fills the rest, so the filled values are only known from this dump.
//! The own `natamux-theme` (S9.2) reads the theme files and takes every key they lack from these
//! rows, so it must give the same values.
//!
//! One file per theme, `tests/golden/theme/<os>/<slug>.json`, one row per value:
//! - `meta`: name, appearance, window background;
//! - `colors`, `status`: every field of `ThemeColors` and `StatusColors`, `[h, s, l, a]` or null
//!   for an unset optional field;
//! - `accents`, `players`: by index;
//! - `syntax`: every entry in order, with colour, weight, style and the other style fields.
//!
//! A theme has one look, so "both modes" is the 7 light and the 10 dark themes.

#![allow(clippy::unwrap_used, clippy::panic)]

use gpui::{HighlightStyle, Hsla, TestAppContext};
use serde_json::{Value, json};
use theme::{Theme, ThemeRegistry};

use super::golden_files;
use super::test_views;
use crate::app::theme_catalog::{ThemeKind, theme_slug};

const SET: &str = "theme";

/// A float as its shortest text, so the file reads well and parses back to the same `f32`.
fn num(value: f32) -> Value {
    json!(value.to_string().parse::<f64>().unwrap())
}

fn hsla(color: Hsla) -> Value {
    json!([num(color.h), num(color.s), num(color.l), num(color.a)])
}

/// Reads `name: Hsla { h: .., s: .., l: .., a: .. }`, `name: Some(Hsla { .. })` and `name: None`
/// fields out of the `Debug` text of a colour struct. The structs have no public list of their
/// fields, so this reads every one of them, and fails on a field of another kind.
fn fields(debug: &str) -> Vec<(String, Value)> {
    let body = debug.split_once(" { ").unwrap().1;
    let mut rest = body.strip_suffix(" }").unwrap();
    let mut out = Vec::new();
    while !rest.is_empty() {
        let (name, after) = rest.split_once(": ").unwrap();
        let (value, after) = if let Some(after) = after.strip_prefix("None") {
            (Value::Null, after)
        } else if let Some(after) = after.strip_prefix("Some(") {
            let (value, after) = hsla_text(after, name);
            (value, after.strip_prefix(')').unwrap())
        } else {
            hsla_text(after, name)
        };
        out.push((name.to_string(), value));
        rest = after.strip_prefix(", ").unwrap_or(after);
    }
    out
}

fn hsla_text<'a>(text: &'a str, name: &str) -> (Value, &'a str) {
    let Some(text) = text.strip_prefix("Hsla { h: ") else {
        panic!("field {name} is not a colour: {text:.60}");
    };
    let mut parts = [0f32; 4];
    let mut text = text;
    for (i, end) in [", s: ", ", l: ", ", a: ", " }"].iter().enumerate() {
        let (number, after) = text.split_once(end).unwrap();
        parts[i] = number.parse().unwrap();
        text = after;
    }
    (json!(parts.map(num)), text)
}

fn style_row(theme: &str, index: usize, name: &str, style: &HighlightStyle) -> Value {
    json!({
        "theme": theme, "group": "syntax", "index": index, "name": name,
        "color": style.color.map(hsla),
        "font_weight": style.font_weight.map(|weight| num(weight.0)),
        "font_style": style.font_style.map(|font_style| format!("{font_style:?}")),
        "background_color": style.background_color.map(hsla),
        "underline": style.underline.map(|underline| format!("{underline:?}")),
        "strikethrough": style.strikethrough.map(|line| format!("{line:?}")),
        "fade_out": style.fade_out.map(num),
    })
}

fn theme_rows(theme: &Theme) -> Vec<Value> {
    let name = theme.name.as_ref();
    let styles = &theme.styles;
    let mut rows = vec![json!({
        "theme": name, "group": "meta",
        "appearance": format!("{:?}", theme.appearance),
        "window_background": format!("{:?}", styles.window_background_appearance),
    })];
    for (group, debug) in [
        ("colors", format!("{:?}", styles.colors)),
        ("status", format!("{:?}", styles.status)),
    ] {
        for (field, value) in fields(&debug) {
            rows.push(json!({"theme": name, "group": group, "field": field, "value": value}));
        }
    }
    for (i, color) in styles.accents.0.iter().enumerate() {
        rows.push(json!({"theme": name, "group": "accents", "index": i, "value": hsla(*color)}));
    }
    for (i, player) in styles.player.0.iter().enumerate() {
        rows.push(json!({
            "theme": name, "group": "players", "index": i,
            "cursor": hsla(player.cursor),
            "background": hsla(player.background),
            "selection": hsla(player.selection),
        }));
    }
    let mut index = 0;
    while let Some(style) = styles.syntax.get(index) {
        let capture = styles.syntax.get_capture_name(index).unwrap();
        rows.push(style_row(name, index, capture, style));
        index += 1;
    }
    rows
}

/// Every theme of the menu, loaded as the app loads them, with the name of its golden file.
fn all(cx: &mut TestAppContext) -> Vec<(String, Vec<Value>)> {
    test_views::init(cx);
    cx.update(|cx| {
        [ThemeKind::Light, ThemeKind::Dark]
            .into_iter()
            .flat_map(|kind| kind.themes())
            .map(|name| {
                let theme = ThemeRegistry::global(cx).get(name).unwrap();
                (theme_slug(name), theme_rows(&theme))
            })
            .collect()
    })
}

/// Writes `$NATAMUX_GOLDEN_OUT/theme/<slug>.json` per theme (`Tool: golden`, plan 8.4).
#[gpui::test]
#[ignore = "makes the golden files; run by the Tool: golden workflow"]
fn golden_theme_values(cx: &mut TestAppContext) {
    for (slug, rows) in all(cx) {
        golden_files::write(SET, &slug, &rows);
    }
}

#[cfg(any(target_os = "linux", target_os = "macos"))]
#[gpui::test]
fn the_themes_hold_what_the_golden_holds(cx: &mut TestAppContext) {
    for (slug, rows) in all(cx) {
        golden_files::check(SET, &slug, &rows);
    }
}

#[gpui::test]
fn every_theme_has_all_colours_and_some_syntax(cx: &mut TestAppContext) {
    let themes = all(cx);
    assert_eq!(themes.len(), 17);
    for (slug, rows) in themes {
        let count = |group: &str| rows.iter().filter(|row| row["group"] == group).count();
        assert_eq!(count("colors"), 143, "{slug}: fields of ThemeColors");
        assert_eq!(count("status"), 42, "{slug}: fields of StatusColors");
        for group in ["accents", "players", "syntax"] {
            assert!(count(group) > 0, "{slug}: no {group}");
        }
    }
}

#[test]
fn reads_colour_fields_from_debug_text() {
    let text = "S { a: Hsla { h: 0.5, s: 0.25, l: 1.0, a: 1.0 }, b: None, \
                c: Some(Hsla { h: 0.0, s: 0.0, l: 0.5, a: 0.125 }) }";
    let got = fields(text);
    assert_eq!(got.len(), 3);
    assert_eq!(got[0], ("a".to_string(), json!([0.5, 0.25, 1.0, 1.0])));
    assert_eq!(got[1], ("b".to_string(), Value::Null));
    assert_eq!(got[2].1, json!([0.0, 0.0, 0.5, 0.125]));
}
