//! The needs-you count on the app's Dock icon, as Electron's `app.setBadgeCount` gave it. GPUI has
//! no API for it, so this calls AppKit. Other systems show no count.

/// `None` takes the label away.
#[cfg(target_os = "macos")]
pub fn set_dock_badge(label: Option<&str>) {
    use objc2::MainThreadMarker;
    use objc2_app_kit::NSApplication;
    use objc2_foundation::NSString;

    // AppKit works on the main thread only; a test thread sets nothing.
    let Some(main_thread) = MainThreadMarker::new() else {
        return;
    };
    let label = label.map(NSString::from_str);
    NSApplication::sharedApplication(main_thread)
        .dockTile()
        .setBadgeLabel(label.as_deref());
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_badge(_label: Option<&str>) {}
