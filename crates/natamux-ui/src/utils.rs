//! Fixed sizes of the platform.

/// The room the macOS window buttons need at the left of the title bar, in px. It does not grow
/// with the UI size. Apps built with the macOS 26 SDK draw the buttons further right (plan D21).
pub const TRAFFIC_LIGHT_PADDING: f32 = if cfg!(macos_sdk_26_or_later) {
    78.
} else {
    71.
};
