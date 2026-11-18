//! The app's asset source: only the app's own files (fonts, icons, themes).

use std::borrow::Cow;

use gpui::{App, AssetSource, Result, SharedString};

use super::asset_table;

pub struct NatamuxAssets;

impl AssetSource for NatamuxAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(asset_table::find(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(asset_table::paths_with_prefix(path)
            .map(SharedString::from)
            .collect())
    }
}

impl NatamuxAssets {
    /// Adds every `.ttf` of `fonts/` to the text system.
    pub fn load_fonts(&self, cx: &App) -> anyhow::Result<()> {
        let mut fonts = Vec::new();
        for path in self.list("fonts")? {
            if path.ends_with(".ttf")
                && let Some(bytes) = self.load(&path)?
            {
                fonts.push(bytes);
            }
        }
        cx.text_system().add_fonts(fonts)
    }
}
