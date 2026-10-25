use natamux_proto::stages::schema::Step;

use super::*;

const NATAMUX: &str = "natamux";
const SITE: &str = "natamux-site";

fn code(error: ClientError) -> (u16, String) {
    match error {
        ClientError::Status { status, message } => (status, message),
        other => panic!("not a status error: {other}"),
    }
}

fn raw_of(fake: &FakeDaemon, repo: &str) -> Map<String, Value> {
    let text = fake.load_settings(repo).unwrap().text.unwrap();
    match serde_json::from_str(&text).unwrap() {
        Value::Object(raw) => raw,
        other => panic!("not an object: {other}"),
    }
}

#[test]
fn natamux_has_a_file_with_the_reference_pipeline() {
    let loaded = FakeDaemon::default().load_settings(NATAMUX).unwrap();
    assert!(loaded.found);
    assert_eq!(loaded.path, "/data/projects/natamux/.natamux.json");
    assert_eq!(loaded.version.as_deref(), Some("v1"));
    let text = loaded.text.unwrap();
    assert!(text.starts_with("{\n  \""), "2-space JSON");
    assert!(text.contains("grep -qi bug .natamux-task/context.md"));
    let ids: Vec<&str> = loaded.settings.stages.iter().map(Step::id).collect();
    assert_eq!(ids[2..4], ["kind", "build"]);
}

#[test]
fn a_project_with_no_file_has_the_default_pipeline() {
    let loaded = FakeDaemon::default().load_settings(SITE).unwrap();
    assert!(!loaded.found);
    assert!(loaded.text.is_none());
    assert_eq!(loaded.path, "/data/projects/natamux-site/.natamux.json");
    assert_eq!(loaded.settings.stages, default_pipeline());
}

#[test]
fn a_save_stores_the_raw_and_bumps_the_version() {
    let fake = FakeDaemon::default();
    let mut raw = raw_of(&fake, NATAMUX);
    raw.insert("tracker".into(), serde_json::json!({ "type": "github" }));
    let saved = fake
        .save_settings(NATAMUX, raw.clone(), Some("v1".into()))
        .unwrap();
    assert_eq!(saved.version.as_deref(), Some("v2"));
    let loaded = fake.load_settings(NATAMUX).unwrap();
    assert_eq!(loaded.version.as_deref(), Some("v2"));
    assert_eq!(raw_of(&fake, NATAMUX), raw);
}

#[test]
fn a_save_without_a_version_overwrites() {
    let fake = FakeDaemon::default();
    fake.move_settings_version(NATAMUX);
    let raw = raw_of(&fake, NATAMUX);
    assert!(fake.save_settings(NATAMUX, raw, None).is_ok());
}

#[test]
fn a_save_of_a_project_with_no_file_creates_it() {
    let fake = FakeDaemon::default();
    let raw = Map::new();
    let saved = fake.save_settings(SITE, raw, Some("none".into())).unwrap();
    assert_eq!(saved.version.as_deref(), Some("v1"));
    assert!(fake.load_settings(SITE).unwrap().found);
}

#[test]
fn an_old_version_is_a_409_and_changes_nothing() {
    let fake = FakeDaemon::default();
    let raw = raw_of(&fake, NATAMUX);
    let mut changed = raw.clone();
    changed.insert("tracker".into(), serde_json::json!({ "type": "github" }));
    fake.save_settings(NATAMUX, raw, Some("v1".into())).unwrap();
    let error = fake
        .save_settings(NATAMUX, changed, Some("v1".into()))
        .unwrap_err();
    let (status, message) = code(error);
    assert_eq!(status, 409);
    assert_eq!(
        message,
        "/data/projects/natamux/.natamux.json changed since you opened it. Reload it, or overwrite it."
    );
    assert_eq!(
        fake.load_settings(NATAMUX).unwrap().version.as_deref(),
        Some("v2")
    );
}

#[test]
fn the_switch_moves_the_version_as_another_client_would() {
    let fake = FakeDaemon::default();
    let raw = raw_of(&fake, NATAMUX);
    fake.move_settings_version(NATAMUX);
    let error = fake
        .save_settings(NATAMUX, raw, Some("v1".into()))
        .unwrap_err();
    assert_eq!(code(error).0, 409);
    assert_eq!(
        fake.load_settings(NATAMUX).unwrap().version.as_deref(),
        Some("v2")
    );
}

#[test]
fn a_pipeline_that_breaks_the_rules_is_a_400() {
    let fake = FakeDaemon::default();
    let mut raw = raw_of(&fake, NATAMUX);
    // The same id twice.
    raw.insert(
        "stages".into(),
        serde_json::json!([
            { "id": "spec", "kind": "agent", "gate": "manual" },
            { "id": "spec", "kind": "agent", "gate": "auto" }
        ]),
    );
    let (status, message) = code(
        fake.save_settings(NATAMUX, raw, Some("v1".into()))
            .unwrap_err(),
    );
    assert_eq!(status, 400);
    assert!(message.starts_with("stages: "), "{message}");
    assert_eq!(
        fake.load_settings(NATAMUX).unwrap().version.as_deref(),
        Some("v1")
    );
}

#[test]
fn a_file_of_the_wrong_shape_is_a_400() {
    let fake = FakeDaemon::default();
    let mut raw = raw_of(&fake, NATAMUX);
    raw.insert("stages".into(), Value::String("none".into()));
    let (status, message) = code(fake.save_settings(NATAMUX, raw, None).unwrap_err());
    assert_eq!(status, 400);
    assert!(message.starts_with("file: "), "{message}");
}
