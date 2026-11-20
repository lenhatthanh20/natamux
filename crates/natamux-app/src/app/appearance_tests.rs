#![allow(clippy::unwrap_used)]

use gpui::{TestAppContext, font};
use serde_json::json;
use theme::ActiveTheme;

use super::*;
use crate::app::test_views;
use crate::components::colors::CLEAR;

// Plan 6.1: the own values equal the theme settings for the UI and the buffer, and the terminal
// font is the user's whenever they set one, at each UI size.
#[gpui::test]
fn the_own_fonts_and_sizes_equal_the_theme_settings(cx: &mut TestAppContext) {
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
                let settings = theme::theme_settings(cx);
                let case = format!("{ui_size} {font:?}");
                assert_eq!(own.ui_font, settings.ui_font, "{case}");
                assert_eq!(own.ui_font_size, settings.ui_font_size(cx), "{case}");
                assert_eq!(own.buffer_font, settings.buffer_font, "{case}");
                assert_eq!(
                    own.buffer_font_size,
                    settings.buffer_font_size(cx),
                    "{case}"
                );
                assert_eq!(own.buffer_line_height, settings.line_height(), "{case}");
                let terminal_font = own
                    .terminal_font
                    .as_ref()
                    .map(|(family, size)| (family.to_string(), f32::from(*size)));
                let expected = font.as_ref().map(|font| match font.get("family") {
                    Some(_) => ("Menlo".to_string(), 12.5),
                    None => ("Lilex".to_string(), 15.0),
                });
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
    let settings = ThemeSettings::default();
    assert!(!appearance(&settings, &font, 1.0, None).hide_scrollbars);
    assert!(appearance(&settings, &font, 1.0, Some("grid")).hide_scrollbars);
    assert!(!appearance(&settings, &font, 1.0, Some("terminal-scrollbar")).hide_scrollbars);
}

fn custom(family: &str, size: f32) -> ResolvedFont {
    ResolvedFont {
        family: family.into(),
        size,
        source: FontSource::Custom,
    }
}

fn terminal_of(font: &ResolvedFont) -> Option<(String, f32)> {
    appearance(&ThemeSettings::default(), font, 1.0, None)
        .terminal_font
        .map(|(family, size)| (family.to_string(), f32::from(size)))
}

// The terminal size is the font's own, not scaled; the family is kept as typed.
#[test]
fn takes_the_picked_terminal_font_as_it_is() {
    assert_eq!(
        terminal_of(&custom("Iosevka", 14.0)),
        Some(("Iosevka".into(), 14.0))
    );
    let family = "Fira \"Code\\";
    assert_eq!(
        terminal_of(&custom(family, 13.5)),
        Some((family.into(), 13.5))
    );
}

// A terminal with no font of its own takes the buffer font and its size, so the UI size scales
// it.
#[test]
fn leaves_the_built_in_terminal_font_to_the_buffer_font() {
    let built_in = ResolvedFont {
        family: "Lilex".into(),
        size: 15.0,
        source: FontSource::Default,
    };
    assert_eq!(terminal_of(&built_in), None);
}

// A shot holds the caret solid through `Inputs::shot` (text_view/blink.rs); every shot hides the
// scrollbars of the code views, which show for a moment after a scroll, unless its state ends in
// `-scrollbar` (docs/plan-mit-license.md 7.2). Never in real use.
#[test]
fn shows_the_scrollbars_in_the_scrollbar_shots_only() {
    let font = custom("Lilex", 15.0);
    let settings = ThemeSettings::default();
    let shown = |state: Option<&str>| {
        let look = appearance(&settings, &font, 1.0, state);
        (look.hide_scrollbars, look.always_scrollbars)
    };
    assert_eq!(shown(Some("terminal-scrollbar")), (false, true));
    assert_eq!(shown(Some("changes-diff-scrollbar")), (false, true));
    assert_eq!(shown(Some("workspace-editor-scrollbar")), (false, true));
    assert_eq!(shown(Some("workspace-changes-diff")), (true, false));
    assert_eq!(shown(Some("scrollbar-later")), (true, false));
    assert_eq!(shown(Some("settings")), (true, false));
    assert_eq!(shown(None), (false, false));
}

// T-0316: whitespace shows only with the diff's toggle.
#[gpui::test]
fn shows_whitespace_only_when_asked(cx: &mut TestAppContext) {
    test_views::init(cx);
    for on in [false, true] {
        cx.update(|cx| {
            let mut view = ViewState::default();
            view.diff.show_whitespace = on;
            apply(&view, cx);
            assert_eq!(cx.global::<Appearance>().show_whitespace, on);
        });
    }
}

// The settings the app sets are the ones the theme shows: the picked look and themes, the fonts
// at the UI size, and the code views' track border made clear in the shown theme.
#[gpui::test]
fn apply_sets_the_theme_settings_and_shows_their_theme(cx: &mut TestAppContext) {
    test_views::init(cx);
    cx.update(|cx| {
        let view = ViewState {
            theme: "light".into(),
            ui_size: "large".into(),
            ..ViewState::default()
        };
        apply(&view, cx);
        let shown = theme::theme_settings(cx);
        // The `--theme dark` flag of the test init wins over the stored theme.
        assert_eq!(shown.mode, theme::ThemeMode::Dark);
        assert_eq!(shown.light_theme, "Natamux Light");
        assert_eq!(shown.dark_theme, "Natamux Dark");
        assert_eq!(shown.ui_font.family, ".ZedSans");
        assert_eq!(shown.buffer_font, font(".ZedMono"));
        assert_eq!(f32::from(shown.ui_font_size), 17.2);
        assert_eq!(f32::from(shown.buffer_font_size), 14.0);
        assert_eq!(cx.theme().name.as_ref(), "Natamux Dark");
        assert_eq!(cx.theme().colors().scrollbar_track_border, CLEAR);
    });
}

// T-0355: the zoom keys scale the UI, the buffer and the picked terminal font on top of the UI
// size.
#[gpui::test]
fn the_zoom_level_scales_every_font(cx: &mut TestAppContext) {
    test_views::init(cx);
    cx.update(|cx| {
        let view = ViewState {
            zoom_level: 2,
            terminal_font: Some(json!({ "size": 14 })),
            ..ViewState::default()
        };
        apply(&view, cx);
        let own = cx.global::<Appearance>();
        assert_eq!(f32::from(own.ui_font_size), 19.2);
        assert_eq!(f32::from(own.buffer_font_size), 15.6);
        let terminal = own.terminal_font.as_ref().map(|(_, size)| f32::from(*size));
        assert_eq!(terminal, Some(16.8));
    });
}

/// Kit widgets that read the theme (colours, the UI font and size, the light or dark look).
struct Kit;

impl gpui::Render for Kit {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        use ui::prelude::*;
        let font = theme::setup_ui_font(window, cx);
        let part = |name: &'static str, child: AnyElement| {
            div().debug_selector(move || name.into()).child(child)
        };
        div()
            .font(font)
            .bg(cx.theme().colors().background)
            .child(part("kit-label", Label::new("Plan").into_any_element()))
            .child(part(
                "kit-button",
                Button::new("go", "Go").into_any_element(),
            ))
            .child(part(
                "kit-checkbox",
                ui::Checkbox::new("on", ui::ToggleState::Selected).into_any_element(),
            ))
            .child(part(
                "kit-switch",
                ui::Switch::new("off", ui::ToggleState::Unselected).into_any_element(),
            ))
    }
}

// S9.2: the kit draws under the own theme. A widget that read a theme global nobody sets would
// panic in this draw.
#[gpui::test]
fn the_kit_widgets_draw_under_the_own_theme(cx: &mut TestAppContext) {
    test_views::init(cx);
    let (_kit, cx) = cx.add_window_view(|_, _| Kit);
    cx.run_until_parked();
    for part in ["kit-label", "kit-button", "kit-checkbox", "kit-switch"] {
        assert!(cx.debug_bounds(part).is_some(), "{part} was not drawn");
    }
}
