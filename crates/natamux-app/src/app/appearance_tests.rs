#![allow(clippy::unwrap_used)]

use gpui::TestAppContext;
use serde_json::json;
use settings::Settings;
use theme_settings::ThemeSettings;

use super::*;
use crate::app::test_views;

// Plan 6.1: the own values equal what Zed makes of the settings JSON for the UI and the buffer,
// and the terminal font is the user's only when they name a family, at each UI size.
#[gpui::test]
fn the_own_fonts_and_sizes_equal_zeds(cx: &mut TestAppContext) {
    test_views::init(cx);
    let fonts = [
        None,
        Some(json!({ "size": 15 })),
        Some(json!({ "family": "Menlo", "size": 12.5 })),
    ];
    for ui_size in ["compact", "default", "large"] {
        for font in &fonts {
            cx.update(|cx| {
                let view = ViewState {
                    ui_size: ui_size.into(),
                    terminal_font: font.clone(),
                    ..ViewState::default()
                };
                apply(&view, cx);
                let own = cx.global::<Appearance>().clone();
                let zed = ThemeSettings::get_global(cx);
                let case = format!("{ui_size} {font:?}");
                assert_eq!(own.ui_font, zed.ui_font, "{case}");
                assert_eq!(own.ui_font_size, zed.ui_font_size(cx), "{case}");
                assert_eq!(own.buffer_font, zed.buffer_font, "{case}");
                assert_eq!(own.buffer_font_size, zed.buffer_font_size(cx), "{case}");
                assert_eq!(own.buffer_line_height, zed.line_height(), "{case}");
                let terminal_font = own
                    .terminal_font
                    .as_ref()
                    .map(|(family, size)| (family.to_string(), f32::from(*size)));
                let named = font.as_ref().is_some_and(|font| font.get("family").is_some());
                let expected = named.then(|| ("Menlo".to_string(), 12.5));
                assert_eq!(terminal_font, expected, "{case}");
                assert!(!own.hide_scrollbars, "{case}");
            });
        }
    }
    let sizes: Vec<_> = ["compact", "default", "large"]
        .iter()
        .map(|ui_size| {
            cx.update(|cx| {
                let view = ViewState {
                    ui_size: (*ui_size).into(),
                    ..ViewState::default()
                };
                apply(&view, cx);
                let own = cx.global::<Appearance>();
                (f32::from(own.ui_font_size), f32::from(own.buffer_font_size))
            })
        })
        .collect();
    assert_eq!(sizes, [(14.7, 12.0), (16.0, 13.0), (17.2, 14.0)]);
}

#[test]
fn a_shot_hides_the_scrollbars_unless_its_state_shows_them() {
    let font = terminal_font(None);
    assert!(!appearance(1., &font, None).hide_scrollbars);
    assert!(appearance(1., &font, Some("grid")).hide_scrollbars);
    assert!(!appearance(1., &font, Some("terminal-scrollbar")).hide_scrollbars);
}
