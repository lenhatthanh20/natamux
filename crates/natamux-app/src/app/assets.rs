//! The app's asset source: its own files first, Zed's for the rest (its icons, themes, fonts).

use std::borrow::Cow;

use ::assets::Assets;
use gpui::{App, AssetSource, Result, SharedString};

use super::asset_table;

pub struct NatamuxAssets;

impl AssetSource for NatamuxAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        match asset_table::find(path) {
            Some(bytes) => Ok(Some(Cow::Borrowed(bytes))),
            None => Assets.load(path),
        }
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let mut paths: Vec<SharedString> = asset_table::paths_with_prefix(path)
            .map(SharedString::from)
            .collect();
        paths.extend(Assets.list(path)?);
        Ok(paths)
    }
}

impl NatamuxAssets {
    /// Adds every `.ttf` of `fonts/` (ours and Zed's) to the text system.
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
