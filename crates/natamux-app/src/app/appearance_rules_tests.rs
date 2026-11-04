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
    let theme = ThemeSetting {
        mode,
        names: ThemeNames::default(),
    };
    serde_json::from_str(&user_settings(theme, zoom, terminal, None)).unwrap_or(Value::Null)
}

#[test]
fn builds_the_settings_with_the_theme_the_sizes_and_the_terminal_font() {
    let value = settings("dark", 1.0, &font("Iosevka", 14.0));
    assert_eq!(value["theme"]["mode"], "dark");
    assert_eq!(value["theme"]["light"], "Natamux Light");
    assert_eq!(value["theme"]["dark"], "Natamux Dark");
    assert_eq!(value["ui_font_family"], ".ZedSans");
    assert_eq!(value["buffer_font_family"], ".ZedMono");
    assert_eq!(value["ui_font_size"], 16.0);
    assert_eq!(value["buffer_font_size"], 13.0);
    assert_eq!(value["terminal"]["font_family"], "Iosevka");
    assert_eq!(value["terminal"]["font_size"], 14.0);
}

#[test]
fn gives_zed_the_picked_theme_of_each_look() {
    let theme = ThemeSetting {
        mode: "system",
        names: ThemeNames {
            light: "Catppuccin Latte",
            dark: "Rosé Pine",
        },
    };
    let text = user_settings(theme, 1.0, &font("Lilex", 13.0), None);
    let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
    assert_eq!(value["theme"]["mode"], "system");
    assert_eq!(value["theme"]["light"], "Catppuccin Latte");
    assert_eq!(value["theme"]["dark"], "Rosé Pine");
}

#[test]
fn scales_the_ui_and_buffer_sizes_by_the_zoom_to_one_decimal() {
    let terminal = font("Lilex", 13.0);
    let compact = settings("system", UiSize::Compact.zoom(), &terminal);
    assert_eq!(compact["ui_font_size"], 14.7);
    assert_eq!(compact["buffer_font_size"], 12.0);
    let large = settings("system", UiSize::Large.zoom(), &terminal);
    assert_eq!(large["ui_font_size"], 17.2);
    assert_eq!(large["buffer_font_size"], 14.0);
    // The terminal size is the font's own, not scaled.
    assert_eq!(large["terminal"]["font_size"], 13.0);
}

#[test]
fn escapes_the_font_family() {
    let value = settings("light", 1.0, &font("Fira \"Code\\", 13.5));
    assert_eq!(value["terminal"]["font_family"], "Fira \"Code\\");
    assert_eq!(value["terminal"]["font_size"], 13.5);
}

// Zed's terminal with no font of its own takes the buffer font and its size, so the UI size
// scales it.
#[test]
fn leaves_the_built_in_terminal_font_to_zed() {
    let built_in = ResolvedFont {
        family: "Lilex".into(),
        size: 15.0,
        source: FontSource::Default,
    };
    let value = settings("dark", UiSize::Large.zoom(), &built_in);
    assert!(value["terminal"].get("font_family").is_none());
    assert!(value["terminal"].get("font_size").is_none());
}

// Zed shows an editor's scrollbar for a moment after it scrolls, so a shot caught it or not
// (run 40008781483, the Changes shots).
#[test]
fn hides_the_editor_scrollbars_in_a_shot_only() {
    let font = font("Lilex", 15.0);
    let theme = ThemeSetting {
        mode: "dark",
        names: ThemeNames::default(),
    };
    let shot: Value = serde_json::from_str(&user_settings(theme, 1.0, &font, Some("settings")))
        .unwrap_or(Value::Null);
    assert_eq!(shot["scrollbar"]["show"], "never");
    assert!(settings("dark", 1.0, &font).get("scrollbar").is_none());
}

// The caret of a focused box blinks every 500 ms, so a shot had it in one run and not in the
// next (the search box of the Settings shots, runs 40985373281 and 41004951875).
#[test]
fn holds_the_caret_solid_in_a_shot_only() {
    let font = font("Lilex", 15.0);
    let theme = ThemeSetting {
        mode: "dark",
        names: ThemeNames::default(),
    };
    let shot: Value = serde_json::from_str(&user_settings(theme, 1.0, &font, Some("settings")))
        .unwrap_or(Value::Null);
    assert_eq!(shot["cursor_blink"], false);
    assert!(settings("dark", 1.0, &font).get("cursor_blink").is_none());
}

// Every shot hides the scrollbars, so the states whose name ends in `-scrollbar` show them, to
// pin their look (docs/plan-mit-license.md 7.2). Never in real use.
#[test]
fn shows_the_scrollbars_in_the_scrollbar_shots_only() {
    let font = font("Lilex", 15.0);
    let theme = ThemeSetting {
        mode: "light",
        names: ThemeNames::default(),
    };
    let shown = |state: &str| -> Value {
        let text = user_settings(theme, 1.0, &font, Some(state));
        let value: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
        value["scrollbar"]["show"].clone()
    };
    assert_eq!(shown("terminal-scrollbar"), "always");
    assert_eq!(shown("changes-diff-scrollbar"), "always");
    assert_eq!(shown("workspace-editor-scrollbar"), "always");
    assert_eq!(shown("workspace-changes-diff"), "never");
    assert_eq!(shown("scrollbar-later"), "never");
    let shot: Value = serde_json::from_str(&user_settings(
        theme,
        1.0,
        &font,
        Some("terminal-scrollbar"),
    ))
    .unwrap_or(Value::Null);
    assert_eq!(shot["cursor_blink"], false);
    assert!(settings("light", 1.0, &font).get("scrollbar").is_none());
}
