//! The project routes of screen 7: add a project, list and import GitHub issues. Method, path,
//! query, body (fields not given are left out), the answers, and the daemon's 400 / 409 reasons.

// Test code: a failed unwrap is a failed test. clippy.toml allows it only inside #[test] fns.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod support;

use natamux_client::ClientError;
use natamux_proto::projects::repo::AddRepo;
use serde_json::{Value, json};
use support::{FakeDaemon, runtime, within};

fn input(name: Option<&str>) -> AddRepo {
    AddRepo {
        path: "/r/app".into(),
        name: name.map(str::to_string),
        worktrees_dir: None,
    }
}

fn status(code: u16, message: &str) -> ClientError {
    ClientError::Status {
        status: code,
        message: message.into(),
    }
}

/// A daemon that answers `route` with `status` and `body`, and the client of it.
fn daemon_with(
    rt: &tokio::runtime::Runtime,
    route: &str,
    status: &str,
    body: &str,
) -> (FakeDaemon, natamux_client::Client) {
    let daemon = rt.block_on(FakeDaemon::start());
    daemon.state(|state| state.answer(route, status, body));
    let client = daemon.client();
    (daemon, client)
}

#[test]
fn adds_a_project_with_post_and_the_fields_given() {
    let rt = runtime();
    let body = r#"{"repo":{"id":"app","name":"App","path":"/r/app"}}"#;
    let (daemon, client) = daemon_with(&rt, "POST /repos", "201 Created", body);
    let repo = rt
        .block_on(within(client.add_repo(&input(Some("App")))))
        .unwrap();
    assert_eq!((repo.id.as_str(), repo.name.as_str()), ("app", "App"));
    let (route, sent) = daemon.state(|state| state.last_request());
    assert_eq!(route, "POST /repos");
    assert_eq!(
        serde_json::from_str::<Value>(&sent).unwrap(),
        json!({"path": "/r/app", "name": "App"})
    );
    // Fields not given are not sent.
    rt.block_on(within(client.add_repo(&input(None)))).unwrap();
    let (_, sent) = daemon.state(|state| state.last_request());
    assert_eq!(sent, r#"{"path":"/r/app"}"#);
}

#[test]
fn adding_a_project_keeps_the_400_and_409_messages() {
    let rt = runtime();
    for (code, line, message) in [
        (
            400,
            "400 Bad Request",
            "app is not an absolute path to a folder",
        ),
        (409, "409 Conflict", "/r/app is added already"),
        (
            409,
            "409 Conflict",
            "The worktree folder /w is inside the project",
        ),
    ] {
        let body = json!({ "message": message }).to_string();
        let (_, client) = daemon_with(&rt, "POST /repos", line, &body);
        let error = rt
            .block_on(within(client.add_repo(&input(None))))
            .unwrap_err();
        assert_eq!(error, status(code, message));
    }
}

const ISSUES: &str = r#"{"issues":[
    {"number":12,"title":"Crash","body":"Steps","url":"https://github.com/o/r/issues/12","labels":["bug"],"taskId":"T-0007"},
    {"number":13,"title":"Idea","body":"","url":"https://github.com/o/r/issues/13","labels":[],"taskId":null}]}"#;

#[test]
fn lists_issues_of_the_default_project_with_no_query() {
    let rt = runtime();
    let (daemon, client) = daemon_with(&rt, "GET /github/issues", "200 OK", ISSUES);
    let issues = rt.block_on(within(client.list_issues(None))).unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(issues[0].task_id.as_deref(), Some("T-0007"));
    assert_eq!(issues[1].task_id, None);
    assert_eq!(
        daemon.state(|state| state.last_request().0),
        "GET /github/issues"
    );
}

#[test]
fn lists_issues_of_a_project_with_its_id_in_the_query() {
    let rt = runtime();
    let (daemon, client) = daemon_with(&rt, "GET /github/issues?repo=my-app", "200 OK", ISSUES);
    let issues = rt
        .block_on(within(client.list_issues(Some("my-app"))))
        .unwrap();
    assert_eq!(issues.len(), 2);
    assert_eq!(
        daemon.state(|state| state.last_request().0),
        "GET /github/issues?repo=my-app"
    );
}

#[test]
fn no_gh_is_a_409_with_the_daemons_reason() {
    let rt = runtime();
    let message = "GitHub CLI failed: gh is not installed or not on the PATH";
    let body = json!({ "message": message }).to_string();
    let (_, client) = daemon_with(&rt, "GET /github/issues", "409 Conflict", &body);
    let error = rt.block_on(within(client.list_issues(None))).unwrap_err();
    assert_eq!(error, status(409, message));
}

const IMPORTED: &str =
    r#"{"tasks":[{"id":"T-0009","title":"Crash","updatedAt":"2026-10-22T00:00:00.000Z"}]}"#;

#[test]
fn imports_issues_with_post_the_numbers_and_the_project() {
    let rt = runtime();
    let (daemon, client) = daemon_with(&rt, "POST /github/issues/import", "200 OK", IMPORTED);
    let tasks = rt
        .block_on(within(client.import_issues(&[12, 14], Some("my-app"))))
        .unwrap();
    assert_eq!(tasks[0].id, "T-0009");
    let (route, sent) = daemon.state(|state| state.last_request());
    assert_eq!(route, "POST /github/issues/import");
    assert_eq!(sent, r#"{"numbers":[12,14],"repo":"my-app"}"#);
    // The default project: no `repo` in the body.
    rt.block_on(within(client.import_issues(&[12], None)))
        .unwrap();
    let (_, sent) = daemon.state(|state| state.last_request());
    assert_eq!(sent, r#"{"numbers":[12]}"#);
}

#[test]
fn a_failed_import_keeps_the_daemons_message() {
    let rt = runtime();
    let message = "GitHub CLI failed: not signed in";
    let body = json!({ "message": message }).to_string();
    let (_, client) = daemon_with(&rt, "POST /github/issues/import", "409 Conflict", &body);
    let error = rt
        .block_on(within(client.import_issues(&[1], None)))
        .unwrap_err();
    assert_eq!(error, status(409, message));
}

#[test]
fn a_wrong_token_is_unauthorized() {
    let rt = runtime();
    let daemon = rt.block_on(FakeDaemon::start());
    let wrong = natamux_client::Client::new(daemon.client().config().with_token("nope")).unwrap();
    let error = rt.block_on(within(wrong.list_issues(None))).unwrap_err();
    assert_eq!(error, ClientError::Unauthorized);
}
