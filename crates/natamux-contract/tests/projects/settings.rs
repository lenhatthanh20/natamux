//! 56 `project_settings` and 57 `project_skills`.

use std::time::{Duration, Instant};

use hyper::Method;
use natamux_contract::daemon::{Answer, Body, Daemon, Token};
use natamux_contract::events::Events;
use natamux_contract::transcript::Transcript;
use serde_json::{Value, json};

use crate::{
    PROJECT, add_project, agent_token, both, drop_project, get, kept, project_dir, repos_changed,
};

/// Reads the settings until `done` holds for the answer, at most 20 s. The Node daemon looks at
/// the file once a second, so a file written from outside shows up a moment later.
async fn settings_where(d: &Daemon, done: impl Fn(&Value) -> bool) -> Answer {
    let path = format!("/repos/{PROJECT}/settings");
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        let answer = get(d, &path).await;
        if done(&answer.body) {
            return answer;
        }
        assert!(
            Instant::now() < deadline,
            "{}: the settings never got there: {}",
            d.name,
            answer.body
        );
        tokio::time::sleep(Duration::from_millis(200)).await;
    }
}

/// A `PUT` of the settings with an `If-Match` (or none) and the given token.
async fn put(d: &Daemon, body: Value, version: Option<&str>, token: Token<'_>) -> Answer {
    let headers: Vec<(&str, &str)> = version.map(|v| ("if-match", v)).into_iter().collect();
    let path = format!("/repos/{PROJECT}/settings");
    d.request_with(Method::PUT, &path, Body::Json(body), token, &headers)
        .await
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn project_settings() {
    both(
        "project_settings",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            let project = project_dir(&d);
            let file = format!("{project}/.natamux.json");
            drop_project(&d).await;
            d.exec_in(&format!("rm -f '{file}'")).await;
            let mut events = Events::open(&d, Some(&d.token)).await;
            let added = add_project(&d).await;
            assert_eq!(added.status, 201, "{}: {}", d.name, added.body);
            repos_changed(&d, &mut events, 5611).await;

            let none = settings_where(&d, |body| body["found"] == false).await;
            t.note("get without a file", kept(&none));

            // Unknown keys stay in the file.
            let body =
                json!({ "agents": { "permissionMode": "ask" }, "notes": { "keep": [1, 2] } });
            t.note(
                "put",
                kept(&put(&d, body, Some("none"), Token::Daemon).await),
            );
            let saved = settings_where(&d, |body| body["found"] == true).await;
            t.note("get after put", kept(&saved));
            let version = saved.body["version"]
                .as_str()
                .unwrap_or_default()
                .to_string();

            let stale = put(&d, json!({}), Some("none"), Token::Daemon).await;
            t.note("stale If-Match", kept(&stale));
            let wrong = json!({ "agents": { "permissionMode": "yolo" } });
            t.note(
                "bad permissionMode",
                kept(&put(&d, wrong, Some(&version), Token::Daemon).await),
            );
            let agent = agent_token(&d).await;
            let refused = put(&d, json!({}), Some(&version), Token::Given(&agent)).await;
            t.note("agent token", kept(&refused));
            t.note(
                "after refusals",
                kept(&settings_where(&d, |body| body["found"] == true).await),
            );
            t.note(
                "no event",
                json!(repos_changed(&d, &mut events, 5612).await),
            );

            // A file with a mistake gives the defaults and says why.
            d.exec_in(&format!("printf '%s' '<nope>' > '{file}'")).await;
            let broken = settings_where(&d, |body| body["error"].is_string()).await;
            t.note("not JSON", kept(&broken));
            let first = broken.body["error"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let text = r#"{"agents":{"permissionMode":5},"tracker":{"type":"nope"}}"#;
            d.exec_in(&format!("printf '%s' '{text}' > '{file}'")).await;
            let issues = settings_where(&d, |body| {
                body["error"].is_string() && body["error"].as_str() != Some(first.as_str())
            })
            .await;
            t.note("wrong values", kept(&issues));

            let path = "/repos/nope-project/settings";
            t.note("unknown repository", kept(&get(&d, path).await));
            t.note("bad id", kept(&get(&d, "/repos/No/settings").await));

            d.exec_in(&format!("rm -f '{file}'")).await;
            drop_project(&d).await;
            t
        },
    )
    .await;
}

#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn project_skills() {
    both(
        "project_skills",
        &[],
        |d: Daemon, mut t: Transcript| async move {
            drop_project(&d).await;
            let added = add_project(&d).await;
            assert_eq!(added.status, 201, "{}: {}", d.name, added.body);
            let skills = get(&d, &format!("/repos/{PROJECT}/skills")).await;
            assert_eq!(skills.status, 200, "{}: {}", d.name, skills.body);
            t.note("skills", kept(&skills));
            t.note(
                "unknown repository",
                kept(&get(&d, "/repos/nope-project/skills").await),
            );
            t.note("bad id", kept(&get(&d, "/repos/No/skills").await));
            drop_project(&d).await;
            t
        },
    )
    .await;
}
