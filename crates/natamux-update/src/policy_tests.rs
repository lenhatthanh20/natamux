use natamux_core::egress::{Target, check};
use natamux_core::security::NetworkSettings;

use super::policy::{UPDATES_BY_ADMIN, updates_off_with};

#[test]
fn the_machine_key_turns_updates_off_with_the_administrator_reason() {
    // The user's settings turn updates on and offline mode is off.
    let settings = NetworkSettings::defaults(false);
    assert!(settings.updater);
    assert_eq!(updates_off_with(false, &settings), None);
    assert_eq!(
        updates_off_with(true, &settings).as_deref(),
        Some(UPDATES_BY_ADMIN)
    );
    assert!(UPDATES_BY_ADMIN.contains("administrator"));
    // git and gh are not changed by the key.
    let git = Target::Git {
        url: "https://github.com/o/r.git",
        action: "fetch",
    };
    let gh = Target::Gh {
        host: "github.com",
        action: "read",
    };
    let off = NetworkSettings {
        updater: false,
        ..settings
    };
    assert_eq!(check(&off, &git), Ok(()));
    assert_eq!(check(&off, &gh), Ok(()));
    assert!(check(&off, &Target::Update { host: "github.com" }).is_err());
}
