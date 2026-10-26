use std::fs::{create_dir_all, read_to_string, remove_file, write};
use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, SystemTime};

use natamux_git::test_support::TempDir;
use serde_json::json;

use super::*;
use crate::shared::jobs::Clock;

/// Projects "default" (`<dir>/folder`) and "other" (`<dir>/other`) in a repos.json of their own.
fn repos_with(dir: &Path, clock: Clock) -> Arc<Repos> {
    let data = dir.join("data");
    create_dir_all(&data).unwrap();
    let mut list = Vec::new();
    for (id, folder) in [("default", "folder"), ("other", "other")] {
        create_dir_all(dir.join(folder)).unwrap();
        let path = dir.join(folder).to_string_lossy().to_string();
        list.push(json!({ "id": id, "name": id, "path": path }));
    }
    write(data.join("repos.json"), json!(list).to_string()).unwrap();
    Repos::new(&data, clock)
}

struct Setup {
    base: TempDir,
    repos: Arc<Repos>,
    clock: Arc<AtomicI64>,
}

fn setup() -> Setup {
    let base = TempDir::new("settings");
    let clock = Arc::new(AtomicI64::new(0));
    let ticks = Arc::clone(&clock);
    let repos = repos_with(base.path(), Arc::new(move || ticks.load(Ordering::SeqCst)));
    Setup { base, repos, clock }
}

impl Setup {
    fn file(&self) -> PathBuf {
        self.base.path().join("folder").join(SETTINGS_FILE)
    }

    /// The service looks at the file at most once a second.
    fn later(&self, ms: i64) {
        self.clock.fetch_add(ms, Ordering::SeqCst);
    }

    fn load(&self) -> Value {
        self.repos.load("default").unwrap()
    }

    fn create_issue(&self) -> String {
        self.repos.settings_of(None).tracker.create_issue
    }

    fn written(&self) -> Value {
        serde_json::from_str(&read_to_string(self.file()).unwrap()).unwrap()
    }
}

/// A later modified time, so a rewrite counts as a change on a file system with coarse times.
fn touch(file: &Path, step: u64) {
    let at = SystemTime::now() + Duration::from_secs(5 * step);
    std::fs::File::options()
        .write(true)
        .open(file)
        .unwrap()
        .set_modified(at)
        .unwrap();
}

fn defaults() -> Value {
    settings_json(&default_settings())
}

fn error_of(loaded: &Value) -> &str {
    loaded["error"].as_str().unwrap_or_default()
}

#[test]
fn uses_the_defaults_without_a_file() {
    let setup = setup();
    let expected = json!({
        "settings": defaults(),
        "error": null,
        "found": false,
        "text": null,
        "version": "none",
        "path": setup.file().display().to_string(),
    });
    assert_eq!(setup.load(), expected);
    assert_eq!(
        serde_json::to_string(&setup.load()).unwrap(),
        serde_json::to_string(&expected).unwrap()
    );
}

#[test]
fn reads_a_file_and_fills_what_it_leaves_out() {
    let setup = setup();
    write(
        setup.file(),
        json!({ "tracker": { "createIssue": "never" } }).to_string(),
    )
    .unwrap();
    let loaded = setup.load();
    assert!(loaded["error"].is_null());
    let tracker = &loaded["settings"]["tracker"];
    assert_eq!(
        tracker,
        &json!({ "type": "github", "createIssue": "never", "linkInPr": true })
    );
    assert_eq!(
        tracker.to_string(),
        r#"{"type":"github","createIssue":"never","linkInPr":true}"#
    );
    assert_eq!(loaded["settings"]["stages"], defaults()["stages"]);
    let keys: Vec<&String> = loaded["settings"].as_object().unwrap().keys().collect();
    assert_eq!(keys, ["stages", "context", "tracker", "agents"]);
}

#[test]
fn says_why_a_file_cannot_be_used_and_keeps_the_defaults() {
    let setup = setup();
    write(setup.file(), "{ not json").unwrap();
    assert!(error_of(&setup.load()).contains("is not JSON"));

    write(setup.file(), json!({ "stages": [] }).to_string()).unwrap();
    touch(&setup.file(), 1);
    setup.later(1000);
    let loaded = setup.load();
    assert!(error_of(&loaded).contains("stages"));
    assert_eq!(loaded["settings"], defaults());
}

#[test]
fn reads_the_file_again_after_it_changes() {
    let setup = setup();
    let body = |when: &str| json!({ "tracker": { "createIssue": when } }).to_string();
    write(setup.file(), body("never")).unwrap();
    assert_eq!(setup.create_issue(), "never");
    write(setup.file(), body("on-pr")).unwrap();
    touch(&setup.file(), 1);
    setup.later(1000);
    assert_eq!(setup.create_issue(), "on-pr");
}

#[test]
fn looks_at_the_file_at_most_once_a_second_but_sees_its_own_save_at_once() {
    let setup = setup();
    let body = |when: &str| json!({ "tracker": { "createIssue": when } });
    write(setup.file(), body("never").to_string()).unwrap();
    assert_eq!(setup.create_issue(), "never");
    write(setup.file(), body("on-pr").to_string()).unwrap();
    touch(&setup.file(), 1);

    setup.later(999);
    assert_eq!(setup.create_issue(), "never");
    setup.later(1);
    assert_eq!(setup.create_issue(), "on-pr");

    setup.repos.save("default", &body("never"), None).unwrap();
    assert_eq!(setup.create_issue(), "never");
}

#[test]
fn sees_a_file_that_was_added_or_deleted_after_a_second() {
    let setup = setup();
    assert_eq!(setup.load()["found"], false);
    write(setup.file(), "{}\n").unwrap();
    assert_eq!(setup.load()["found"], false);
    setup.later(1000);
    assert_eq!(setup.load()["found"], true);

    remove_file(setup.file()).unwrap();
    setup.later(1000);
    assert_eq!(setup.load()["found"], false);
}

#[test]
fn keeps_a_second_for_each_repository_on_its_own() {
    let setup = setup();
    assert_eq!(setup.load()["found"], false);
    write(setup.base.path().join("other").join(SETTINGS_FILE), "{}\n").unwrap();
    assert_eq!(setup.repos.load("other").unwrap()["found"], true);
}

#[test]
fn saves_valid_settings_as_written_and_refuses_others() {
    let setup = setup();
    let raw = json!({ "stages": [{ "id": "implement", "kind": "agent", "gate": "auto" }] });
    let (settings, _) = setup.repos.save("default", &raw, None).unwrap();
    assert_eq!(settings["stages"], raw["stages"]);
    assert_eq!(setup.written(), raw);
    let empty = setup.repos.save("default", &json!({ "stages": [] }), None);
    assert!(matches!(empty, Err(RepoError::SettingsInput(_))));
    let yolo = json!({ "agents": { "permissionMode": "yolo" } });
    let Err(RepoError::SettingsInput(message)) = setup.repos.save("default", &yolo, None) else {
        panic!("a bad mode must be refused");
    };
    assert!(message.contains("agents.permissionMode"), "{message}");
}

#[test]
fn gives_the_text_of_the_file_and_a_version_that_changes_with_it() {
    let setup = setup();
    let text = "{ \"tracker\": { \"createIssue\": \"never\" }, \"team\": \"web\" }\n";
    write(setup.file(), text).unwrap();
    let first = setup.load();
    assert_eq!(first["text"], text);
    assert_ne!(first["version"], "none");
    assert_eq!(first["version"].as_str().unwrap().len(), 16);

    write(setup.file(), "{}\n").unwrap();
    touch(&setup.file(), 1);
    setup.later(1000);
    assert_ne!(setup.load()["version"], first["version"]);
}

#[test]
fn refuses_a_save_over_a_file_that_changed_since_its_version_was_read() {
    let setup = setup();
    write(setup.file(), "{}\n").unwrap();
    let version = setup.load()["version"].as_str().unwrap().to_string();
    write(
        setup.file(),
        "{ \"tracker\": { \"createIssue\": \"never\" } }\n",
    )
    .unwrap();

    let ask = json!({ "agents": { "permissionMode": "ask" } });
    let Err(RepoError::SettingsChanged(message)) =
        setup.repos.save("default", &ask, Some(&version))
    else {
        panic!("a changed file must be refused");
    };
    assert_eq!(
        message,
        format!(
            "{} changed since you opened it. Reload it, or overwrite it.",
            setup.file().display()
        )
    );
    assert!(read_to_string(setup.file()).unwrap().contains("never"));
    // Without a version the save replaces the file, as "Overwrite" does.
    setup.repos.save("default", &ask, None).unwrap();
    assert!(read_to_string(setup.file()).unwrap().contains("ask"));
}

#[test]
fn takes_the_version_of_a_missing_file_and_refuses_it_once_the_file_is_there() {
    let setup = setup();
    setup
        .repos
        .save("default", &json!({}), Some("none"))
        .unwrap();
    let again = setup.repos.save("default", &json!({}), Some("none"));
    assert!(matches!(again, Err(RepoError::SettingsChanged(_))));
}

#[test]
fn refuses_a_save_over_a_file_that_was_deleted_and_allows_one_over_a_file_only_touched() {
    let setup = setup();
    write(setup.file(), "{}\n").unwrap();
    let version = setup.load()["version"].as_str().unwrap().to_string();
    touch(&setup.file(), 1);
    assert!(
        setup
            .repos
            .save("default", &json!({}), Some(&version))
            .is_ok()
    );

    let saved = setup.load()["version"].as_str().unwrap().to_string();
    remove_file(setup.file()).unwrap();
    let gone = setup.repos.save("default", &json!({}), Some(&saved));
    assert!(matches!(gone, Err(RepoError::SettingsChanged(_))));
}

#[test]
fn says_why_an_empty_file_or_one_that_is_not_an_object_cannot_be_used() {
    let setup = setup();
    write(setup.file(), "").unwrap();
    assert!(error_of(&setup.load()).contains("is not JSON"));
    write(setup.file(), "[]").unwrap();
    touch(&setup.file(), 1);
    setup.later(1000);
    let loaded = setup.load();
    assert!(!loaded["error"].is_null());
    assert_eq!(loaded["settings"], defaults());
}

#[test]
fn keeps_the_fields_of_a_stage_that_the_screen_does_not_show() {
    let setup = setup();
    let stages = json!([
        { "id": "implement", "kind": "agent", "gate": "auto", "requireCommit": true,
          "check": "make test" },
        { "id": "qa", "kind": "manual", "gate": "manual", "owner": "sam" },
    ]);
    setup
        .repos
        .save("default", &json!({ "stages": stages }), None)
        .unwrap();
    assert_eq!(setup.written()["stages"], stages);
}

#[test]
fn keeps_fields_it_does_not_know() {
    let setup = setup();
    let raw = json!({ "team": "web", "tracker": { "createIssue": "never" } });
    let (settings, version) = setup.repos.save("default", &raw, None).unwrap();
    assert_eq!(setup.written(), raw);
    assert!(settings.get("team").is_none());
    // The key order and the 2-space layout of the body stay, with a closing newline.
    let text = read_to_string(setup.file()).unwrap();
    assert_eq!(
        text,
        "{\n  \"team\": \"web\",\n  \"tracker\": {\n    \"createIssue\": \"never\"\n  }\n}\n"
    );
    assert_eq!(version, version_of(Some(&text)));
}

// Node's zod says which field of the step is wrong; Rust names the step (see the Gate's table).
#[test]
fn names_the_step_that_does_not_read_as_a_stage_group_or_branch() {
    let setup = setup();
    let body = json!({ "stages": [
        { "id": "implement", "kind": "agent", "gate": "auto" },
        { "kind": "agent", "gate": "auto" },
    ] });
    let Err(RepoError::SettingsInput(message)) = setup.repos.save("default", &body, None) else {
        panic!("a step without an id is refused");
    };
    assert_eq!(message, "stages.1: Invalid input");
    write(setup.file(), body.to_string()).unwrap();
    assert_eq!(error_of(&setup.load()), "stages.1: Invalid input");
}

#[test]
fn writes_numbers_and_number_keys_as_json_stringify_does() {
    let setup = setup();
    let raw: Value =
        serde_json::from_str(r#"{ "extra": { "b": 1.0, "1": 1e3, "c": [2.50] } }"#).unwrap();
    let (_, version) = setup.repos.save("default", &raw, None).unwrap();
    let text = read_to_string(setup.file()).unwrap();
    assert_eq!(
        text,
        "{\n  \"extra\": {\n    \"1\": 1000,\n    \"b\": 1,\n    \"c\": [\n      2.5\n    ]\n  }\n}\n"
    );
    assert_eq!(version, version_of(Some(&text)));
}

#[test]
fn starts_agents_in_auto_mode_when_the_file_has_a_bad_mode() {
    let setup = setup();
    let body = json!({ "agents": { "permissionMode": "yolo" } });
    write(setup.file(), body.to_string()).unwrap();
    let loaded = setup.load();
    assert!(error_of(&loaded).contains("agents.permissionMode"));
    assert_eq!(loaded["settings"]["agents"]["permissionMode"], "auto");
}

#[test]
fn reads_the_most_send_backs_and_has_no_limit_without_it() {
    let setup = setup();
    assert_eq!(setup.repos.settings_of(None).agents.max_send_backs, None);
    assert!(
        setup.load()["settings"]["agents"]
            .get("maxSendBacks")
            .is_none()
    );

    write(setup.file(), r#"{ "agents": { "maxSendBacks": 0 } }"#).unwrap();
    touch(&setup.file(), 1);
    setup.later(1000);
    assert_eq!(setup.repos.settings_of(None).agents.max_send_backs, Some(0));
    assert_eq!(setup.load()["settings"]["agents"]["maxSendBacks"], 0);

    write(setup.file(), r#"{ "agents": { "maxSendBacks": -1 } }"#).unwrap();
    touch(&setup.file(), 2);
    setup.later(1000);
    let loaded = setup.load();
    assert!(error_of(&loaded).contains("agents.maxSendBacks"));
    assert_eq!(setup.repos.settings_of(None).agents.max_send_backs, None);
}

#[test]
fn gives_tasks_of_a_removed_repository_the_defaults_but_does_not_load_or_save_it() {
    let setup = setup();
    assert_eq!(setup.repos.settings_of(Some("gone")), default_settings());
    assert!(matches!(
        setup.repos.load("gone"),
        Err(RepoError::NotFound(_))
    ));
    assert!(matches!(
        setup.repos.save("gone", &json!({}), None),
        Err(RepoError::NotFound(_))
    ));
}

#[test]
fn the_version_is_the_first_16_hex_digits_of_the_sha_256() {
    assert_eq!(version_of(None), "none");
    assert_eq!(version_of(Some("")), "e3b0c44298fc1c14");
}
