//! Every theme the app ships, by name.

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::{Context as _, Result};
use gpui::{App, AssetSource, Global, SharedString};

use crate::loader::load_family;
use crate::theme::Theme;

#[derive(Default)]
pub struct ThemeRegistry {
    themes: HashMap<SharedString, Arc<Theme>>,
}

struct GlobalThemeRegistry(Arc<ThemeRegistry>);

impl Global for GlobalThemeRegistry {}

impl ThemeRegistry {
    /// The themes `init` loaded; none before it.
    pub fn global(cx: &App) -> Arc<Self> {
        cx.try_global::<GlobalThemeRegistry>()
            .map(|registry| registry.0.clone())
            .unwrap_or_default()
    }

    pub(crate) fn set_global(self, cx: &mut App) {
        cx.set_global(GlobalThemeRegistry(Arc::new(self)));
    }

    /// Every theme of the `.json` files in `themes/`.
    pub fn from_assets(assets: &dyn AssetSource) -> Result<Self> {
        let mut themes = HashMap::new();
        for path in assets.list("themes/")? {
            if !path.ends_with(".json") {
                continue;
            }
            let bytes = assets.load(&path)?.with_context(|| format!("no {path}"))?;
            for theme in load_family(&bytes).with_context(|| format!("theme file {path}"))? {
                themes.insert(theme.name.clone(), Arc::new(theme));
            }
        }
        Ok(ThemeRegistry { themes })
    }

    pub fn get(&self, name: &str) -> Result<Arc<Theme>> {
        self.themes
            .get(name)
            .cloned()
            .with_context(|| format!("theme not found: {name}"))
    }

    /// The names in sorted order.
    pub fn list_names(&self) -> Vec<SharedString> {
        let mut names: Vec<_> = self.themes.keys().cloned().collect();
        names.sort();
        names
    }
}
