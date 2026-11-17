use gpui::{SharedString, font};

use super::*;

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

#[test]
fn builds_the_settings_with_the_theme_and_the_sizes() {
    let value = theme_settings(ThemeChoice::Dark, ThemeNames::default(), 1.0);
    assert_eq!(value.mode, ThemeMode::Dark);
    assert_eq!(value.light_theme, "Natamux Light");
    assert_eq!(value.dark_theme, "Natamux Dark");
    assert_eq!(value.ui_font.family, ".ZedSans");
    assert_eq!(value.buffer_font, font(".ZedMono"));
    assert_eq!(f32::from(value.ui_font_size), 16.0);
    assert_eq!(f32::from(value.buffer_font_size), 13.0);
    let modes: Vec<_> = ThemeChoice::ALL.iter().map(|choice| choice.mode()).collect();
    assert_eq!(modes, [ThemeMode::System, ThemeMode::Light, ThemeMode::Dark]);
}

#[test]
fn gives_the_picked_theme_of_each_look() {
    let names = ThemeNames {
        light: "Catppuccin Latte",
        dark: "Rosé Pine",
    };
    let value = theme_settings(ThemeChoice::System, names, 1.0);
    assert_eq!(value.mode, ThemeMode::System);
    assert_eq!(value.light_theme, "Catppuccin Latte");
    assert_eq!(value.dark_theme, "Rosé Pine");
}

#[test]
fn scales_the_ui_and_buffer_sizes_by_the_zoom_to_one_decimal() {
    let sizes = |size: UiSize| {
        let value = theme_settings(ThemeChoice::System, ThemeNames::default(), size.zoom());
        (f32::from(value.ui_font_size), f32::from(value.buffer_font_size))
    };
    assert_eq!(sizes(UiSize::Compact), (14.7, 12.0));
    assert_eq!(sizes(UiSize::Large), (17.2, 14.0));
}

// The JSON tab's bar drew a track border line (T-0294).
#[test]
fn hides_the_scrollbar_track_border_of_the_code_views() {
    for choice in ThemeChoice::ALL {
        let value = theme_settings(choice, ThemeNames::default(), 1.0);
        let clear = (SharedString::from("scrollbar_track_border"), CLEAR);
        assert_eq!(value.color_overrides, vec![clear]);
    }
}
