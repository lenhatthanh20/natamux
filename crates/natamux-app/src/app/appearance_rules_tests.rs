use serde_json::Value;

use super::*;
use crate::terminals::terminal_font::FontSource;

#[test]
fn keeps_light_and_dark_and_treats_anything_else_as_system() {
    assert_eq!(parse_theme("light"), ThemeChoice::Light);
    assert_eq!(parse_theme("dark"), ThemeChoice::Dark);
    assert_eq!(parse_theme(""), ThemeChoice::System);
    assert_eq!(parse_theme("blue"), ThemeChoice::System);
}

#[test]
fn follows_the_system_only_for_the_system_choice() {
    assert!(resolve_theme(ThemeChoice::System, true));
    assert!(!resolve_theme(ThemeChoice::System, false));
    assert!(!resolve_theme(ThemeChoice::Light, true));
    assert!(resolve_theme(ThemeChoice::Dark, false));
}

#[test]
fn names_and_labels_the_choices() {
    let names: Vec<_> = ThemeChoice::ALL.iter().map(|c| c.name()).collect();
    assert_eq!(names, ["system", "light", "dark"]);
    let labels: Vec<_> = ThemeChoice::ALL.iter().map(|c| c.label()).collect();
    assert_eq!(labels, ["System", "Light", "Dark"]);
    for choice in ThemeChoice::ALL {
        assert_eq!(parse_theme(choice.name()), choice);
    }
}

#[test]
fn reads_the_stored_ui_size_and_falls_back_to_the_default() {
    assert_eq!(parse_ui_size("compact"), UiSize::Compact);
    assert_eq!(parse_ui_size("large"), UiSize::Large);
    assert_eq!(parse_ui_size("huge"), UiSize::Default);
    assert_eq!(parse_ui_size(""), UiSize::Default);
}

#[test]
fn maps_the_sizes_to_the_zoom_of_the_spec() {
    assert_eq!(UiSize::Compact.zoom(), 0.92);
    assert_eq!(UiSize::Default.zoom(), 1.0);
    assert_eq!(UiSize::Large.zoom(), 1.077);
    let labels: Vec<_> = UiSize::ALL.iter().map(|size| size.label()).collect();
    assert_eq!(labels, ["Compact", "Default", "Large"]);
    for size in UiSize::ALL {
        assert_eq!(parse_ui_size(size.name()), size);
    }
}

fn font(family: &str, size: f32) -> ResolvedFont {
    ResolvedFont {
        family: family.into(),
        size,
        source: FontSource::Custom,
    }
}

fn settings(mode: &str, zoom: f32, terminal: &ResolvedFont) -> Value {
    serde_json::from_str(&user_settings(mode, zoom, terminal)).unwrap_or(Value::Null)
}

#[test]
fn builds_the_settings_with_the_theme_the_sizes_and_the_terminal_font() {
    let value = settings("dark", 1.0, &font("Iosevka", 14.0));
    assert_eq!(value["theme"]["mode"], "dark");
    assert_eq!(value["theme"]["light"], "Natamux Light");
    assert_eq!(value["theme"]["dark"], "Natamux Dark");
    assert_eq!(value["ui_font_family"], "Inter");
    assert_eq!(value["buffer_font_family"], "JetBrains Mono");
    assert_eq!(value["ui_font_size"], 16.0);
    assert_eq!(value["buffer_font_size"], 15.0);
    assert_eq!(value["terminal"]["font_family"], "Iosevka");
    assert_eq!(value["terminal"]["font_size"], 14.0);
}

#[test]
fn scales_the_ui_and_buffer_sizes_by_the_zoom_to_one_decimal() {
    let terminal = font("JetBrains Mono", 13.0);
    let compact = settings("system", UiSize::Compact.zoom(), &terminal);
    assert_eq!(compact["ui_font_size"], 14.7);
    assert_eq!(compact["buffer_font_size"], 13.8);
    let large = settings("system", UiSize::Large.zoom(), &terminal);
    assert_eq!(large["ui_font_size"], 17.2);
    assert_eq!(large["buffer_font_size"], 16.2);
    // The terminal size is the font's own, not scaled.
    assert_eq!(large["terminal"]["font_size"], 13.0);
}

#[test]
fn escapes_the_font_family() {
    let value = settings("light", 1.0, &font("Fira \"Code\\", 13.5));
    assert_eq!(value["terminal"]["font_family"], "Fira \"Code\\");
    assert_eq!(value["terminal"]["font_size"], 13.5);
}
