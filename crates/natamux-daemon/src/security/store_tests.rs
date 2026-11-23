use std::sync::Mutex;

use natamux_core::security::{OUTSIDE_CHANGE_WARNING, SecurityMode};
use serde_json::json;

use super::*;

fn folder(name: &str) -> std::path::PathBuf {
    let root = std::env::temp_dir().join(format!("natamux-store-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    root
}

#[test]
fn a_save_body_needs_strict_and_refuses_looser_keys() {
    assert!(checked_text(&json!([])).is_err());
    assert!(checked_text(&json!({})).is_err());
    assert!(checked_text(&json!({ "strict": "on" })).is_err());
    let looser = checked_text(&json!({ "strict": true, "ciPush": "snapshot" })).unwrap_err();
    assert!(looser.contains("ciPush"), "{looser}");
    let text = checked_text(&json!({ "strict": true, "ciPush": "forbid" })).unwrap();
    assert!(text.contains("\"forbid\""), "{text}");
    let off = checked_text(&json!({ "strict": false })).unwrap();
    assert_eq!(
        read_value(&serde_json::from_str(&off).unwrap()).mode,
        SecurityMode::Personal
    );
}

#[test]
fn the_body_names_each_key_with_blocked_and_reason() {
    let personal = security_body(&SecurityState::default());
    assert_eq!(personal["strict"], false);
    assert_eq!(personal["keys"]["ciPush"]["blocked"], false);
    assert!(personal["keys"]["ciPush"]["reason"].is_null());
    assert!(personal["warning"].is_null());
    let strict = security_body(&read_value(&json!({ "strict": true })));
    assert_eq!(strict["strict"], true);
    assert_eq!(strict["keys"]["agentAutoMode"]["value"], "forbid");
    assert_eq!(strict["keys"]["agentAutoMode"]["blocked"], true);
    assert!(strict["keys"]["agentAutoMode"]["reason"].is_string());
    assert_eq!(strict["keys"]["ciPush"]["value"], "approved-commit");
}

#[test]
fn tells_each_change_once_and_an_outside_edit_with_its_warning() {
    let root = folder("tell");
    let store = SecurityStore::open(&root);
    let bus = EventBus::new();
    let got: Arc<Mutex<Vec<String>>> = Arc::default();
    let sink = Arc::clone(&got);
    bus.subscribe(move |message| sink.lock().unwrap().push(message.to_json()));
    store.tell(&bus);
    assert!(got.lock().unwrap().is_empty());
    store.save(&json!({ "strict": true })).unwrap();
    store.tell(&bus);
    store.tell(&bus);
    assert_eq!(
        got.lock().unwrap().clone(),
        [r#"{"kind":"security.changed","strict":true,"warning":null}"#]
    );
    std::fs::write(root.join("security.json"), r#"{"strict": false}"#).unwrap();
    store.source.check_now();
    store.tell(&bus);
    let last = got.lock().unwrap().last().cloned().unwrap();
    assert!(
        last.contains("\"strict\":true") && last.contains(OUTSIDE_CHANGE_WARNING),
        "{last}"
    );
}
