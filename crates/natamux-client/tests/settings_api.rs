//! The settings routes: path, method, `If-Match` header, body, answer, and the two refusals the
//! editor tells apart (409, a file that changed; 400, a rule the daemon checks).

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_client::ClientError;
use serde_json::{Map, Value, json};
use support::{FakeDaemon, runtime, within};

const SAVED: &str =
    r#"{"settings":{"stages":[{"id":"spec","kind":"agent","gate":"manual"}]},"version":"v2"}"#;

fn raw() -> Map<String, Value> {
    json!({"tracker": {"type": "none"}, "stages": [{"id": "spec", "kind": "agent", "gate": "manual"}]})
        .as_object()
        .unwrap()
        .clone()
}

/// Runs `save_settings` against a daemon that answers `status` and `body`, and gives back the
/// answer, the method and path, the `If-Match` header and the body the client sent.
fn save(
    version: Option<&str>,
    status: &str,
    body: &str,
) -> (
    Result<natamux_proto::projects::repo::SavedSettings, ClientError>,
    String,
    Option<String>,
    String,
) {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer("PUT /repos/my-app/settings", status, body));
    let client = daemon.client();
    let answer = rt.block_on(within(client.save_settings(
        "my-app",
        raw(),
        version.map(str::to_string),
    )));
    daemon.state(|state| {
        let (route, sent) = state.last_request();
        (answer, route, state.last_header("if-match"), sent)
    })
}

#[test]
fn saves_with_put_the_raw_body_and_the_version_in_if_match() {
    let (answer, route, if_match, sent) = save(Some("v1-AbC"), "200 OK", SAVED);
    let saved = answer.unwrap();
    assert_eq!(route, "PUT /repos/my-app/settings");
    assert_eq!(if_match.as_deref(), Some("v1-AbC"));
    assert_eq!(saved.version.as_deref(), Some("v2"));
    assert_eq!(saved.settings.stages.len(), 1);
    // The key order of the file stays: tracker, then stages.
    assert!(sent.find("tracker").unwrap() < sent.find("stages").unwrap());
    assert_eq!(
        serde_json::from_str::<Value>(&sent).unwrap(),
        Value::Object(raw())
    );
}

#[test]
fn saves_with_no_if_match_when_there_is_no_version() {
    let (answer, _, if_match, _) = save(None, "200 OK", SAVED);
    assert!(answer.is_ok());
    assert_eq!(if_match, None);
}

#[test]
fn a_file_that_changed_on_disk_is_a_conflict() {
    let message = "The settings file changed since you opened it";
    let body = format!(r#"{{"message":"{message}"}}"#);
    let (answer, ..) = save(Some("old"), "409 Conflict", &body);
    let error = answer.unwrap_err();
    assert!(error.is_conflict());
    assert_eq!(
        error,
        ClientError::Status {
            status: 409,
            message: message.into()
        }
    );
}

#[test]
fn a_rule_the_daemon_checks_keeps_its_message_and_is_no_conflict() {
    let message = "stages: A pipeline has at least one stage";
    let body = format!(r#"{{"message":"{message}"}}"#);
    let (answer, ..) = save(Some("v1"), "400 Bad Request", &body);
    let error = answer.unwrap_err();
    assert!(!error.is_conflict());
    assert_eq!(
        error,
        ClientError::Status {
            status: 400,
            message: message.into()
        }
    );
    assert!(!ClientError::Unauthorized.is_conflict());
}

#[test]
fn reads_the_text_and_the_version_of_the_file() {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    let body = r#"{"settings":{},"error":null,"found":true,"text":"{\n  \"tracker\": {}\n}","version":"v1-AbC","path":"/r/.natamux.json"}"#;
    daemon.state(|state| state.answer("GET /repos/my-app/settings", "200 OK", body));
    let client = daemon.client();
    let loaded = rt.block_on(within(client.load_settings("my-app"))).unwrap();
    assert_eq!(loaded.text.as_deref(), Some("{\n  \"tracker\": {}\n}"));
    assert_eq!(loaded.version.as_deref(), Some("v1-AbC"));
    let none = r#"{"settings":{},"error":null,"found":false,"text":null,"version":"none","path":"/r/.natamux.json"}"#;
    daemon.state(|state| state.answer("GET /repos/my-app/settings", "200 OK", none));
    let loaded = rt.block_on(within(client.load_settings("my-app"))).unwrap();
    assert_eq!(
        (loaded.text, loaded.version.as_deref()),
        (None, Some("none"))
    );
}
