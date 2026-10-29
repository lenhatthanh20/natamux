//! The needs-you count on the app's Dock icon, as Electron's `app.setBadgeCount` gave it. GPUI has
//! no API for it, so this calls AppKit. Other systems show no count.
//!
//! macOS hides the label of an app that asked for notifications without the badge option, and
//! GPUI asks for alerts and sounds only. So the app asks itself, with badges, before GPUI posts
//! its first note: the first request decides what the prompt grants.

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

/// Alerts and sounds for GPUI's notes, and the badge for the Dock count.
#[cfg(target_os = "macos")]
fn badge_permission() -> objc2_user_notifications::UNAuthorizationOptions {
    use objc2_user_notifications::UNAuthorizationOptions;

    UNAuthorizationOptions::Alert | UNAuthorizationOptions::Sound | UNAuthorizationOptions::Badge
}

/// Asks macOS for notifications with badges. The first time, macOS shows its prompt; later it
/// shows none.
#[cfg(target_os = "macos")]
pub fn ask_for_badges() {
    use block2::RcBlock;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError};
    use objc2_user_notifications::UNUserNotificationCenter;

    // Outside an app bundle (a dev run, a test) the notification center aborts the process.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }
    // macOS keeps the answer; the app sets its label either way.
    let completion = RcBlock::new(|_: Bool, _: *mut NSError| {});
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(badge_permission(), &completion);
}

#[cfg(not(target_os = "macos"))]
pub fn ask_for_badges() {}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use objc2_user_notifications::UNAuthorizationOptions;

    use super::badge_permission;

    #[test]
    fn asks_for_the_badge_with_alerts_and_sounds() {
        let options = badge_permission();
        assert!(options.contains(UNAuthorizationOptions::Badge));
        assert!(options.contains(UNAuthorizationOptions::Alert));
        assert!(options.contains(UNAuthorizationOptions::Sound));
    }

    #[test]
    fn asks_nothing_outside_an_app_bundle() {
        // The test binary has no bundle: this must return, not abort.
        super::ask_for_badges();
    }
}
