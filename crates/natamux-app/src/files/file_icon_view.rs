//! The icon of a file in the Files tab, the open-file tabs, the editor header and the Changes
//! tab: the Material Icon Theme icon of `file_icons.rs`, as the web's `FileIcon.tsx` shows it.
//! The icons keep their own colours, so they are images, not tinted SVGs (as `AgentIcon`).

use gpui::{App, Img, img};
use ui::prelude::*;

use super::file_icons::file_icon_name;

/// The bundled icon of a Material name.
pub fn file_icon_path(name: &str) -> String {
    format!("icons/files/{name}.svg")
}

/// The icon of a path at 12 px, in the light version when the window is light.
pub fn file_icon(path: &str, cx: &App) -> Img {
    let light = cx.theme().appearance.is_light();
    img(SharedString::from(file_icon_path(file_icon_name(
        path, light,
    ))))
    .flex_none()
    .size(rems_from_px(12.0_f32))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::asset_table::find;
    use crate::files::file_icons::KNOWN_ICONS;

    #[test]
    fn every_known_name_is_bundled() {
        for name in KNOWN_ICONS {
            let path = file_icon_path(name);
            assert!(
                find(&path).is_some_and(|bytes| !bytes.is_empty()),
                "{path} missing"
            );
        }
    }
}
