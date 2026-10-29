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

// The `UNAuthorizationOptions` bits, plain so the test of the permission runs on every system.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BADGE: usize = 1 << 0;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const SOUND: usize = 1 << 1;
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const ALERT: usize = 1 << 2;

/// Alerts and sounds for GPUI's notes, and the badge for the Dock count.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
const BADGE_PERMISSION: usize = ALERT | SOUND | BADGE;

/// Asks macOS for notifications with badges. The first time, macOS shows its prompt; later it
/// shows none.
#[cfg(target_os = "macos")]
pub fn ask_for_badges() {
    use block2::RcBlock;
    use objc2::MainThreadMarker;
    use objc2::runtime::Bool;
    use objc2_foundation::{NSBundle, NSError};
    use objc2_user_notifications::{UNAuthorizationOptions, UNUserNotificationCenter};

    // The prompt belongs to the app's main thread; a test thread asks nothing.
    if MainThreadMarker::new().is_none() {
        return;
    }
    // Outside an app bundle (a dev run, a test) the notification center aborts the process.
    if NSBundle::mainBundle().bundleIdentifier().is_none() {
        return;
    }
    // macOS keeps the answer; the app sets its label either way.
    let completion = RcBlock::new(|granted: Bool, error: *mut NSError| {
        if !error.is_null() {
            eprintln!("natamux-app: the notification permission request failed");
        } else if !granted.as_bool() {
            eprintln!("natamux-app: notifications not allowed; the Dock shows no count");
        }
    });
    let options = UNAuthorizationOptions::from_bits_retain(BADGE_PERMISSION);
    UNUserNotificationCenter::currentNotificationCenter()
        .requestAuthorizationWithOptions_completionHandler(options, &completion);
}

#[cfg(not(target_os = "macos"))]
pub fn ask_for_badges() {}

#[cfg(test)]
mod tests {
    use super::{ALERT, BADGE, BADGE_PERMISSION, SOUND};

    #[test]
    fn asks_for_the_badge_with_alerts_and_sounds() {
        assert_eq!(BADGE_PERMISSION & BADGE, BADGE);
        assert_eq!(BADGE_PERMISSION & ALERT, ALERT);
        assert_eq!(BADGE_PERMISSION & SOUND, SOUND);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn uses_the_bits_of_the_user_notifications_framework() {
        use objc2_user_notifications::UNAuthorizationOptions;

        assert_eq!(
            UNAuthorizationOptions::from_bits_retain(BADGE_PERMISSION),
            UNAuthorizationOptions::Alert
                | UNAuthorizationOptions::Sound
                | UNAuthorizationOptions::Badge
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn asks_nothing_outside_an_app_bundle() {
        // A test runs off the main thread and outside a bundle: this must return, not abort.
        super::ask_for_badges();
    }
}
