use super::preferences::{auto_update, set_auto_update};

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
