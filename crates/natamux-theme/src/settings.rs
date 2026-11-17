//! Which theme shows, and the fonts and sizes of the UI and of code.

use gpui::{App, Font, FontFeatures, Global, Hsla, Pixels, SharedString, Window, font, px};

use crate::theme::Appearance;

pub const DEFAULT_LIGHT_THEME: &str = "Natamux Light";
pub const DEFAULT_DARK_THEME: &str = "Natamux Dark";

/// The UI font size at zoom 1; the UI size setting scales it.
pub const BASE_UI_FONT_SIZE: f32 = 16.;
/// The code font size at zoom 1. Code is 13, so it sits beside the 13 px rows of the lists.
pub const BASE_BUFFER_FONT_SIZE: f32 = 13.;

/// Which look shows: the system's, or always light or dark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ThemeMode {
    #[default]
    System,
    Light,
    Dark,
}

/// The line height of code, as a factor of the font size.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BufferLineHeight {
    #[default]
    Comfortable,
    Standard,
    Custom(f32),
}

impl BufferLineHeight {
    pub fn value(&self) -> f32 {
        match self {
            BufferLineHeight::Comfortable => 1.618,
            BufferLineHeight::Standard => 1.3,
            BufferLineHeight::Custom(value) => *value,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ThemeSettings {
    pub mode: ThemeMode,
    /// The theme of the light look.
    pub light_theme: SharedString,
    /// The theme of the dark look.
    pub dark_theme: SharedString,
    pub ui_font: Font,
    pub ui_font_size: Pixels,
    pub buffer_font: Font,
    pub buffer_font_size: Pixels,
    pub buffer_line_height: BufferLineHeight,
    /// Colours the shown theme takes in place of its own, by `ThemeColors` field name.
    pub color_overrides: Vec<(SharedString, Hsla)>,
}

impl Global for ThemeSettings {}

impl Default for ThemeSettings {
    fn default() -> Self {
        ThemeSettings::at_zoom(1.)
    }
}

/// `base` times `zoom`, rounded to 0.1 px and kept within 6 to 100 px.
pub fn zoomed(base: f32, zoom: f32) -> Pixels {
    let size = (f64::from(base) * f64::from(zoom) * 10.).round() / 10.;
    px((size as f32).clamp(6., 100.))
}

impl ThemeSettings {
    /// The app's fonts at a UI zoom (0.92, 1 or 1.077 for the three UI sizes).
    pub fn at_zoom(zoom: f32) -> Self {
        ThemeSettings {
            mode: ThemeMode::default(),
            light_theme: DEFAULT_LIGHT_THEME.into(),
            dark_theme: DEFAULT_DARK_THEME.into(),
            ui_font: Font {
                features: FontFeatures::disable_ligatures(),
                ..font(".ZedSans")
            },
            ui_font_size: zoomed(BASE_UI_FONT_SIZE, zoom),
            buffer_font: font(".ZedMono"),
            buffer_font_size: zoomed(BASE_BUFFER_FONT_SIZE, zoom),
            buffer_line_height: BufferLineHeight::Comfortable,
            color_overrides: Vec::new(),
        }
    }

    pub fn ui_font(&self, _cx: &App) -> &Font {
        &self.ui_font
    }

    pub fn buffer_font(&self, _cx: &App) -> &Font {
        &self.buffer_font
    }

    pub fn ui_font_size(&self, _cx: &App) -> Pixels {
        self.ui_font_size
    }

    pub fn buffer_font_size(&self, _cx: &App) -> Pixels {
        self.buffer_font_size
    }

    pub fn line_height(&self) -> f32 {
        self.buffer_line_height.value()
    }

    /// The look that shows while the system's is `system`.
    pub fn appearance(&self, system: Appearance) -> Appearance {
        match self.mode {
            ThemeMode::System => system,
            ThemeMode::Light => Appearance::Light,
            ThemeMode::Dark => Appearance::Dark,
        }
    }

    /// The theme of `look`.
    pub fn theme_name(&self, look: Appearance) -> &SharedString {
        match look {
            Appearance::Light => &self.light_theme,
            Appearance::Dark => &self.dark_theme,
        }
    }
}

/// The settings `init` set.
pub fn theme_settings(cx: &App) -> &ThemeSettings {
    cx.global::<ThemeSettings>()
}

/// Sets the rem size of `window` to the UI font size and gives the UI font.
pub fn setup_ui_font(window: &mut Window, cx: &mut App) -> Font {
    let settings = theme_settings(cx);
    window.set_rem_size(settings.ui_font_size);
    settings.ui_font.clone()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_at_the_three_zooms() {
        let sizes: Vec<_> = [0.92, 1., 1.077]
            .into_iter()
            .map(ThemeSettings::at_zoom)
            .map(|settings| {
                (
                    f32::from(settings.ui_font_size),
                    f32::from(settings.buffer_font_size),
                )
            })
            .collect();
        assert_eq!(sizes, [(14.7, 12.0), (16.0, 13.0), (17.2, 14.0)]);
    }

    #[test]
    fn sizes_stay_within_6_and_100() {
        assert_eq!(zoomed(16., 0.1), px(6.));
        assert_eq!(zoomed(16., 10.), px(100.));
        assert_eq!(zoomed(13., 1.0384), px(13.5));
    }

    #[test]
    fn the_fonts_and_line_height() {
        let settings = ThemeSettings::default();
        assert_eq!(settings.ui_font.family, ".ZedSans");
        assert_eq!(settings.ui_font.features, FontFeatures::disable_ligatures());
        assert_eq!(settings.ui_font.fallbacks, None);
        assert_eq!(settings.buffer_font, font(".ZedMono"));
        assert_eq!(settings.line_height(), 1.618);
        assert_eq!(BufferLineHeight::Standard.value(), 1.3);
        assert_eq!(BufferLineHeight::Custom(2.).value(), 2.);
    }

    #[test]
    fn the_mode_picks_the_look_and_the_look_picks_the_theme() {
        let mut settings = ThemeSettings {
            light_theme: "Ayu Light".into(),
            ..ThemeSettings::default()
        };
        assert_eq!(settings.appearance(Appearance::Light), Appearance::Light);
        assert_eq!(settings.appearance(Appearance::Dark), Appearance::Dark);
        settings.mode = ThemeMode::Light;
        assert_eq!(settings.appearance(Appearance::Dark), Appearance::Light);
        assert_eq!(settings.theme_name(Appearance::Light), "Ayu Light");
        settings.mode = ThemeMode::Dark;
        assert_eq!(settings.appearance(Appearance::Light), Appearance::Dark);
        assert_eq!(settings.theme_name(Appearance::Dark), DEFAULT_DARK_THEME);
    }
}
