use super::preferences::{auto_update, load_offline, offline, set_auto_update};

#[test]
fn stores_the_switch_without_losing_other_settings() {
    let folder = std::env::temp_dir().join(format!("natamux-update-prefs-{}", std::process::id()));
    let file = folder.join("settings.json");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(&file, r#"{"repoDir":"/work","autoUpdate":false}"#).expect("settings");
    assert!(!super::preferences::load(&file).expect("load"));
    super::preferences::save(&file, true).expect("save");
    assert!(super::preferences::load(&file).expect("load"));
    assert!(
        std::fs::read_to_string(&file)
            .expect("read")
            .contains("/work")
    );
    std::fs::remove_dir_all(folder).expect("cleanup");
}

#[test]
fn automatic_checks_are_off_unless_the_saved_value_is_true() {
    assert!(!auto_update("{}"));
    assert!(!auto_update("{\"autoUpdate\":1}"));
    assert!(!auto_update("not json"));
    assert!(auto_update("{\"autoUpdate\":true}"));
}

#[test]
fn saving_the_preference_keeps_other_settings() {
    let saved = set_auto_update("{\"theme\":\"dark\"}", true).expect("settings");
    let value: serde_json::Value = serde_json::from_str(&saved).expect("valid json");
    assert_eq!(value["theme"], "dark");
    assert_eq!(value["autoUpdate"], true);
}

#[test]
fn offline_is_off_unless_the_saved_value_is_true() {
    assert!(!offline("{}"));
    assert!(!offline("{\"offline\":1}"));
    assert!(!offline("{\"offline\":\"true\"}"));
    assert!(!offline("{\"offline\":false,\"autoUpdate\":true}"));
    assert!(!offline("not json"));
    assert!(offline("{\"autoUpdate\":false,\"offline\":true}"));
}

#[test]
fn offline_file_missing_or_unreadable_is_off() {
    let folder = std::env::temp_dir().join(format!("natamux-offline-{}", std::process::id()));
    let file = folder.join("settings.json");
    std::fs::create_dir_all(&folder).expect("folder");
    assert!(!load_offline(&file));
    std::fs::write(&file, "{\"offline\":true}").expect("settings");
    assert!(load_offline(&file));
    std::fs::write(&file, "{").expect("settings");
    assert!(!load_offline(&file));
    std::fs::remove_dir_all(folder).expect("cleanup");
}
