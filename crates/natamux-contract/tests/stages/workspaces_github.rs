//! 45 `push_and_pull_request`, 46 `issues`, 47 `fan_out` and 48 `ci_wait`, in the `workspaces`
//! module (the CI filter `workspaces::`). The fake `gh` answers; its calls are compared.

use natamux_contract::daemon::{Daemon, daemons};
use natamux_contract::transcript::{Transcript, compare};
use serde_json::json;

use super::{
    alias, branch_of, gh_writes, make_task, open_workspace, repo, reset_gh, retire, shaless,
    wait_exec, wait_type, workspace_of,
};
use crate::{
    both, cancel, get, kept, post, put, report, stage_events, start, task, twin, wait_event,
};

const DRAFT: &str = r#"{"state":"OPEN","isDraft":true,"statusCheckRollup":[]}"#;

const ISSUES: &str = r#"[{"number":1,"title":"Fixed bug one","body":"The first fixed issue.","url":"https://github.com/natamux/contract/issues/1","labels":[{"name":"bug"}]},{"number":2,"title":"Fixed bug two","body":"The second fixed issue.","url":"https://github.com/natamux/contract/issues/2","labels":[]}]"#;

/// `POST /push` (`git.pushed`, the branch is on the bare origin), `POST /pull-request`
/// (`pr.opened`, the fake URL; the project makes the issue first, so the body ends with its
/// closing line), 409 for a second one, `POST /pull-request/refresh` (`prStatus` from
/// `pr-state.json`), and the `gh` calls that wrote something are equal.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn push_and_pull_request() {
    both(
        "push_and_pull_request",
        4501,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            reset_gh(&d).await;
            let before = d.fake_gh_calls().await;
            make_task(&d, 4501, "Pull request").await;
            let ws = open_workspace(&d, &id).await;
            d.exec_in(&format!("cd '{ws}' && printf 'feature\\n' > feature.txt"))
                .await;
            let base = format!("/tasks/{id}");
            let message = json!({ "message": "Add feature" });
            t.note(
                "commit",
                kept(&post(&d, &format!("{base}/commit"), message).await),
            );
            t.note(
                "push",
                kept(&post(&d, &format!("{base}/push"), json!({})).await),
            );
            let branch = branch_of(&d, &id).await;
            let remote = d
                .git_in(&ws, &format!("ls-remote --heads origin '{branch}'"))
                .await;
            t.note("on origin", json!(shaless(&remote)));
            let pr = format!("{base}/pull-request");
            t.note("pull request", kept(&post(&d, &pr, json!({})).await));
            t.note("second pull request", kept(&post(&d, &pr, json!({})).await));
            d.write_fake_gh("pr-state.json", DRAFT).await;
            let refresh = post(&d, &format!("{pr}/refresh"), json!({})).await;
            t.note("refresh", kept(&refresh));
            t.note("events", stage_events(&d, &id).await);
            t.note("gh calls", gh_writes(&d, &before).await);
            retire(&d, &id).await;
            t
        },
    )
    .await;
}

/// `GET /github/issues`, `POST /github/issues/import` (tasks; again, none), `POST
/// /tasks/:id/issue` (linked, `issue.created`), 409 when linked already, 400 without numbers.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn issues() {
    both(
        "issues",
        4601,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            reset_gh(&d).await;
            d.write_fake_gh("issues.json", ISSUES).await;
            let before = d.fake_gh_calls().await;
            let list = format!("/github/issues?repo={}", repo());
            t.note("issues", kept(&get(&d, &list).await));
            let numbers = json!({ "numbers": [1, 2], "repo": repo() });
            let imported = post(&d, "/github/issues/import", numbers.clone()).await;
            // The daemon numbers these tasks itself, so the ids differ by daemon.
            let made = imported.body["tasks"].as_array().into_iter().flatten();
            for (index, each) in made.enumerate() {
                let label = format!("<imported {}>", index + 1);
                alias(&mut t, each["id"].as_str().unwrap_or("none"), &label);
            }
            t.note("import", kept(&imported));
            t.note(
                "import again",
                kept(&post(&d, "/github/issues/import", numbers).await),
            );
            t.note("issues after", kept(&get(&d, &list).await));
            let none = post(&d, "/github/issues/import", json!({ "numbers": [] })).await;
            t.note("no numbers", kept(&none));
            make_task(&d, 4601, "Issue").await;
            let path = format!("/tasks/{id}/issue");
            t.note("create issue", kept(&post(&d, &path, json!({})).await));
            t.note("linked already", kept(&post(&d, &path, json!({})).await));
            t.note("events", stage_events(&d, &id).await);
            t.note("gh calls", gh_writes(&d, &before).await);
            retire(&d, &id).await;
            t
        },
    )
    .await;
}

/// `POST /tasks/:id/fan-out`: count 2 makes `(try 2)` and `(try 3)` with the `tries-<id>` label;
/// with `start` the try gets a workspace and an agent; count 6 is 400.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn fan_out() {
    both(
        "fan_out",
        4701,
        &[],
        |d: Daemon, id, mut t: Transcript| async move {
            make_task(&d, 4701, "Fan out").await;
            let path = format!("/tasks/{id}/fan-out");
            let mut tries: Vec<String> = Vec::new();
            for (count, start) in [(2, false), (1, true)] {
                let answer = post(&d, &path, json!({ "count": count, "start": start })).await;
                let made = answer.body["tasks"].as_array().into_iter().flatten();
                for each in made {
                    // Tries are numbered from 2; the daemon makes their ids.
                    let each = each["id"].as_str().unwrap_or("none").to_string();
                    alias(&mut t, &each, &format!("<try {}>", tries.len() + 2));
                    tries.push(each);
                }
                t.note(&format!("fan out {count}, start {start}"), kept(&answer));
                t.note("source", kept(&get(&d, &format!("/tasks/{id}")).await));
            }
            t.note(
                "count 6",
                kept(&post(&d, &path, json!({ "count": 6 })).await),
            );
            for each in tries.iter().chain([&id]) {
                retire(&d, each).await;
            }
            t
        },
    )
    .await;
}

/// One task through its CI wait on `build`. `pass` ends at `stage.checked` with the `CI passed:`
/// line; the other is sent back with `CI ci failure:` and the log. Then the task is cancelled
/// and its CI branch leaves the origin.
async fn ci_round(d: &Daemon, t: &mut Transcript, id: &str, pass: bool) {
    let label = if pass { "pass" } else { "fail" };
    let base = format!("/tasks/{id}");
    let on = put(
        d,
        &format!("{base}/ci"),
        json!({ "stage": "build", "on": true }),
    )
    .await;
    t.note(&format!("{label}: ci on"), kept(&on));
    t.note(&format!("{label}: start"), start(d, id).await);
    wait_event(d, id, "stage.started", "build", 1).await;
    let ws = workspace_of(d, id).await;
    let log = format!("cd '{ws}' && git log --format=%s | grep -q '^Add feature' && echo yes");
    wait_exec(d, &log, "the commit of the agent").await;
    t.note(
        &format!("{label}: report"),
        report(d, id, "build", "1").await,
    );
    wait_event(d, id, "stage.ci-started", "build", 1).await;
    if pass {
        wait_event(d, id, "stage.checked", "build", 1).await;
    } else {
        wait_type(d, id, "stage.reopened", 1).await;
        wait_event(d, id, "stage.started", "build", 2).await;
    }
    t.note(&format!("{label}: events"), stage_events(d, id).await);
    t.note(&format!("{label}: task"), task(d, id).await);
    let branch = branch_of(d, id).await;
    let remote = format!("ls-remote --heads origin 'natamux-ci/{branch}'");
    t.note(
        &format!("{label}: ci branch"),
        json!(shaless(&d.git_in(&ws, &remote).await)),
    );
    cancel(d, id).await;
    let gone = format!("cd '{ws}' && test -z \"$(git {remote})\" && echo yes");
    wait_exec(d, &gone, "the CI branch deleted").await;
}

/// The two rounds on one daemon: the first passes, the second has a failing run.
async fn ci_flow(d: Daemon) -> Transcript {
    let (pass, fail) = (twin(&d, 4801), twin(&d, 4802));
    let mut t = Transcript::new(d.name);
    alias(&mut t, &pass, "<task>");
    alias(&mut t, &fail, "<task 2>");
    reset_gh(&d).await;
    let before = d.fake_gh_calls().await;
    ci_round(&d, &mut t, &pass, true).await;
    d.write_fake_gh("conclusion", "failure").await;
    d.write_fake_gh("log-failed.txt", "boom: the test failed")
        .await;
    ci_round(&d, &mut t, &fail, false).await;
    t.note("gh calls", gh_writes(&d, &before).await);
    t
}

/// `PUT /tasks/:id/ci` on `build`; once the stage passes its checks, `stage.ci-started`; the
/// snapshot is on `natamux-ci/<branch>` of the origin; the fake run passes and `stage.checked`
/// has the `CI passed:` line; a second task with `conclusion: failure` is sent back with `CI ci
/// failure:` and the log; cancelling deletes the CI branch. The tasks and worktrees are made
/// on Node, then on Rust; only the CI waits (the daemon polls every 15 s) run side by side.
#[tokio::test]
#[ignore = "needs both daemons: the daemon-contract CI job"]
async fn ci_wait() {
    let (node, rust) = daemons();
    for d in [&node, &rust] {
        for number in [4801, 4802] {
            let id = make_task(d, number, "CI wait").await;
            open_workspace(d, &id).await;
        }
    }
    let (on_node, on_rust) = tokio::join!(ci_flow(node.clone()), ci_flow(rust.clone()));
    compare("ci_wait", &on_node, &on_rust, &[]);
    for d in [&node, &rust] {
        for number in [4801, 4802] {
            retire(d, &twin(d, number)).await;
        }
    }
}
