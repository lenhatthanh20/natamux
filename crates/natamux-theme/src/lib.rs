//! Natamux's own theme: the theme files of the app, the light or dark look, and the fonts and
//! sizes of the UI (docs/plan-mit-license.md 6.8, S9). The names match the ones the app's views
//! already read through `theme::`, so the views keep their paths once the app names this crate
//! `theme` in Cargo.
//!
//! A theme file sets only some colours. Every colour it lacks comes from `fallbacks.tsv`: the
//! final value of that colour for each bundled theme, so no rule makes them up.

mod colors;
mod loader;
mod registry;
mod settings;
mod syntax;
mod theme;

use std::sync::Arc;

use anyhow::Result;
use gpui::{App, AssetSource, Global, Hsla, SharedString};

pub use colors::{AccentColors, PlayerColor, PlayerColors, StatusColors, ThemeColors};
pub use registry::ThemeRegistry;
pub use settings::{
    BASE_BUFFER_FONT_SIZE, BASE_UI_FONT_SIZE, BufferLineHeight, DEFAULT_DARK_THEME,
    DEFAULT_LIGHT_THEME, ThemeMode, ThemeSettings, setup_ui_font, theme_settings, zoomed,
};
pub use syntax::SyntaxTheme;
pub use theme::{Appearance, Theme, ThemeStyles};

/// The look of the system. The app keeps it current as the window's look changes.
#[derive(Clone, Copy, Debug)]
pub struct SystemAppearance(pub Appearance);

impl Default for SystemAppearance {
    fn default() -> Self {
        SystemAppearance(Appearance::Dark)
    }
}

impl std::ops::Deref for SystemAppearance {
    type Target = Appearance;

    fn deref(&self) -> &Appearance {
        &self.0
    }
}

#[derive(Default)]
struct GlobalSystemAppearance(SystemAppearance);

impl Global for GlobalSystemAppearance {}

impl SystemAppearance {
    pub fn init(cx: &mut App) {
        let look = SystemAppearance(cx.window_appearance().into());
        cx.set_global(GlobalSystemAppearance(look));
    }

    pub fn global(cx: &App) -> Self {
        cx.try_global::<GlobalSystemAppearance>()
            .map(|look| look.0)
            .unwrap_or_default()
    }

    pub fn global_mut(cx: &mut App) -> &mut Self {
        &mut cx.default_global::<GlobalSystemAppearance>().0
    }
}

/// The theme that shows. Views that keep colours observe it.
pub struct GlobalTheme {
    theme: Arc<Theme>,
    /// The name and the overrides it was made from.
    made_from: (SharedString, Vec<(SharedString, Hsla)>),
}

impl Global for GlobalTheme {}

impl GlobalTheme {
    pub fn theme(cx: &App) -> &Arc<Theme> {
        &cx.global::<GlobalTheme>().theme
    }
}

pub trait ActiveTheme {
    fn theme(&self) -> &Arc<Theme>;
}

impl ActiveTheme for App {
    fn theme(&self) -> &Arc<Theme> {
        GlobalTheme::theme(self)
    }
}

/// Loads the theme files of `assets` and shows the theme the settings pick (the defaults at zoom 1
/// until `set_appearance`).
pub fn init(assets: impl AssetSource, cx: &mut App) -> Result<()> {
    SystemAppearance::init(cx);
    if !cx.has_global::<ThemeSettings>() {
        cx.set_global(ThemeSettings::default());
    }
    ThemeRegistry::from_assets(&assets)?.set_global(cx);
    reload_theme(cx);
    Ok(())
}

/// `init` in the dark look, so tests see Natamux Dark.
pub fn init_for_tests(assets: impl AssetSource, cx: &mut App) -> Result<()> {
    init(assets, cx)?;
    set_appearance(cx, |settings| settings.mode = ThemeMode::Dark);
    Ok(())
}

/// Changes the settings, then shows the theme they pick.
pub fn set_appearance(cx: &mut App, change: impl FnOnce(&mut ThemeSettings)) {
    let mut settings = cx
        .try_global::<ThemeSettings>()
        .cloned()
        .unwrap_or_default();
    change(&mut settings);
    cx.set_global(settings);
    reload_theme(cx);
}

/// Shows the theme the settings pick in the current system look. Nothing changes while that name
/// and the overrides stay the same. A name the registry lacks shows the default of its look.
pub fn reload_theme(cx: &mut App) {
    let Some(settings) = cx.try_global::<ThemeSettings>() else {
        return;
    };
    let look = settings.appearance(SystemAppearance::global(cx).0);
    let made_from = (
        settings.theme_name(look).clone(),
        settings.color_overrides.clone(),
    );
    if cx
        .try_global::<GlobalTheme>()
        .is_some_and(|shown| shown.made_from == made_from)
    {
        return;
    }
    let registry = ThemeRegistry::global(cx);
    let default = match look {
        Appearance::Light => DEFAULT_LIGHT_THEME,
        Appearance::Dark => DEFAULT_DARK_THEME,
    };
    let Ok(theme) = registry
        .get(&made_from.0)
        .or_else(|_| registry.get(default))
    else {
        return;
    };
    let theme = if made_from.1.is_empty() {
        theme
    } else {
        let mut theme = (*theme).clone();
        for (field, color) in &made_from.1 {
            theme.styles.colors.set(field, *color);
        }
        Arc::new(theme)
    };
    cx.set_global(GlobalTheme { theme, made_from });
    // Cached views keep their last frame until a refresh; they must take the new colours.
    cx.refresh_windows();
}
