//! The window's identity on Linux: the app id that matches `natamux.desktop` (Wayland app id,
//! X11 `WM_CLASS`), and the X11 window icon. Windows reads the icon from the exe (`build.rs`).

/// Must equal the desktop file name and its `StartupWMClass` (scripts/package-rust-desktop.sh).
pub const APP_ID: &str = "natamux";

/// For window managers that find no desktop file, such as an AppImage run as is.
#[cfg(target_os = "linux")]
pub fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    // 256 px, not the 1024 px icon: X11 would get a 4 MB property, past its request size.
    const PNG: &[u8] = include_bytes!("../../resources/icon-256.png");
    match image::load_from_memory_with_format(PNG, image::ImageFormat::Png) {
        Ok(icon) => Some(std::sync::Arc::new(icon.to_rgba8())),
        Err(error) => {
            crate::app::app_log::write(&format!("window icon not loaded: {error}"));
            None
        }
    }
}

#[cfg(test)]
#[path = "window_icon_tests.rs"]
mod tests;
