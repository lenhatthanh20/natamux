//! `issue-linker.test.ts`.

use std::time::Duration;

use natamux_daemon::github::issue_linker::{
    IssueLinker, IssueLinkerOptions, LinkError, closing_line, issue_url,
};
use natamux_daemon::tasks::helpers::legacy_pipeline;
use natamux_proto::tasks::task::{IssueLink, Task};
use natamux_store::task_files::archive_dir;
use serde_json::json;

use super::{Calls, Fx, jobs, recording, repo_path};

const ISSUE: &str = "https://github.com/o/r/issues/12\n";

fn linker_with(fx: &Fx, gh: natamux_git::gh::RunGh) -> IssueLinker {
    let linker = IssueLinker::new(IssueLinkerOptions {
        tasks: fx.tasks.clone(),
        bus: std::sync::Arc::clone(&fx.bus),
        data_dir: fx.dir.0.clone(),
        repo_path: repo_path(),
        settings_of: fx.settings_of(),
        gh,
        jobs: jobs(),
    });
    linker.start();
    linker
}

fn linker(fx: &Fx) -> (IssueLinker, Calls) {
    let (gh, calls) = recording(|_| Ok(ISSUE.to_string()));
    (linker_with(fx, gh), calls)
}

fn create(fx: &Fx, input: serde_json::Value) -> Task {
    fx.tasks.create_task(Some(&input), &[], true).unwrap()
}

fn at_spec_review(fx: &Fx) -> Task {
    let task = create(fx, json!({ "title": "Export tasks" }));
    fx.tasks
        .apply_event(
            &task.id,
            Some(&json!({
                "source": "human",
                "type": "state.overridden",
                "payload": { "to": "review", "stage": "spec" },
            })),
        )
        .unwrap();
    std::fs::create_dir_all(archive_dir(&fx.dir.0, &task.id)).unwrap();
    std::fs::write(
        archive_dir(&fx.dir.0, &task.id).join("spec.md"),
        "# Goal\nExport tasks as CSV.\n",
    )
    .unwrap();
    fx.tasks.get_task(&task.id).unwrap()
}

fn approve_spec(fx: &Fx, task: &Task) {
    fx.tasks
        .apply_event(
            &task.id,
            Some(&json!({
                "source": "human",
                "type": "stage.approved",
                "payload": { "stage": "spec" },
            })),
        )
        .unwrap();
}

/// The project asks for an issue on its own; by default it does not.
fn opens_issues(fx: &Fx, create_issue: &str) {
    fx.settings.lock().unwrap().tracker.create_issue = create_issue.into();
}

fn legacy_stages(fx: &Fx) {
    fx.settings.lock().unwrap().stages = legacy_pipeline();
}

fn link(fx: &Fx, id: &str, number: u32) {
    let link = json!({
        "provider": "github",
        "key": format!("#{number}"),
        "url": format!("https://github.com/o/r/issues/{number}"),
    });
    fx.tasks.link_issue(id, &link, "human").unwrap();
}

#[test]
fn opens_no_issue_by_default_on_spec_approval_or_with_the_pull_request() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    let task = at_spec_review(&fx);
    approve_spec(&fx, &task);
    linker.settle();
    legacy_stages(&fx);
    let other = create(
        &fx,
        json!({ "title": "Export", "description": "Adds a CSV export." }),
    );

    assert_eq!(
        linker.pull_request_body(&other).unwrap(),
        "Adds a CSV export."
    );
    assert!(calls.lock().unwrap().is_empty());
    assert_eq!(fx.tasks.get_task(&task.id).unwrap().issue, None);
}

#[test]
fn opens_an_issue_with_the_spec_when_the_spec_is_approved() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    opens_issues(&fx, "on-spec-approved");
    let task = at_spec_review(&fx);
    approve_spec(&fx, &task);
    linker.settle();

    let calls = calls.lock().unwrap();
    assert_eq!(
        calls[0].args[..4],
        ["issue", "create", "--title", "Export tasks"]
    );
    assert!(calls[0].args[5].contains("Export tasks as CSV."));
    assert!(calls[0].args[5].contains(&format!("Made by Natamux for {}.", task.id)));
    assert_eq!(
        fx.tasks.get_task(&task.id).unwrap().issue,
        Some(IssueLink {
            provider: "github".into(),
            key: "#12".into(),
            url: "https://github.com/o/r/issues/12".into(),
        })
    );
}

#[test]
fn opens_no_issue_when_the_project_does_not_want_one_or_the_task_has_one() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    opens_issues(&fx, "manual");
    let task = at_spec_review(&fx);
    approve_spec(&fx, &task);
    linker.settle();
    assert!(calls.lock().unwrap().is_empty());

    linker.create(&task.id).unwrap();
    assert!(matches!(
        linker.create(&task.id).unwrap_err(),
        LinkError::Link(_)
    ));
}

#[test]
fn opens_one_issue_for_two_requests_at_the_same_time() {
    let fx = Fx::new();
    let (gh, calls) = recording(|_| {
        std::thread::sleep(Duration::from_millis(300));
        Ok(ISSUE.to_string())
    });
    let linker = linker_with(&fx, gh);
    let task = create(&fx, json!({ "title": "Export" }));

    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| linker.create(&task.id));
        std::thread::sleep(Duration::from_millis(100));
        let second = scope.spawn(|| linker.create(&task.id));
        (first.join().unwrap(), second.join().unwrap())
    });

    let (first, second) = (first.unwrap(), second.unwrap());
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(
        first.issue.as_ref().map(|issue| issue.key.as_str()),
        Some("#12")
    );
    assert_eq!(second, first);
}

#[test]
fn a_panic_in_create_now_resolves_its_flight() {
    let fx = Fx::new();
    let panicked = std::sync::atomic::AtomicBool::new(false);
    let (gh, _calls) = recording(move |_| {
        if !panicked.swap(true, std::sync::atomic::Ordering::SeqCst) {
            panic!("gh broke");
        }
        Ok(ISSUE.to_string())
    });
    let linker = std::sync::Arc::new(linker_with(&fx, gh));
    let task = create(&fx, json!({ "title": "Export" }));

    let leader = {
        let (linker, id) = (std::sync::Arc::clone(&linker), task.id.clone());
        std::thread::spawn(move || linker.create(&id))
    };
    assert!(leader.join().is_err());

    let (sender, receiver) = std::sync::mpsc::channel();
    let id = task.id.clone();
    std::thread::spawn(move || {
        let _ = sender.send(linker.create(&id).is_ok());
    });
    assert!(
        receiver.recv_timeout(Duration::from_secs(10)).is_ok(),
        "the second request hung on the flight of the panic"
    );
}

#[test]
fn does_not_link_a_new_issue_over_one_linked_while_it_was_opened() {
    let fx = Fx::new();
    let task = create(&fx, json!({ "title": "Export" }));
    // Node linked it between the call and the answer of gh; gh is the only gap here.
    let tasks = fx.tasks.clone();
    let id = task.id.clone();
    let (gh, _) = recording(move |_| {
        let link = json!({
            "provider": "github",
            "key": "#3",
            "url": "https://github.com/o/r/issues/3",
        });
        tasks.link_issue(&id, &link, "human").unwrap();
        Ok(ISSUE.to_string())
    });
    let linker = linker_with(&fx, gh);

    let error = linker.create(&task.id).unwrap_err();

    assert!(error.to_string().contains("linked to #3"), "{error}");
    let linked = fx.tasks.get_task(&task.id).unwrap().issue;
    assert_eq!(linked.map(|issue| issue.key), Some("#3".to_string()));
}

#[test]
fn ends_the_pull_request_body_with_the_closing_line() {
    let fx = Fx::new();
    let (linker, _) = linker(&fx);
    let task = create(
        &fx,
        json!({ "title": "Export", "description": "Adds a CSV export." }),
    );
    link(&fx, &task.id, 3);

    let body = linker
        .pull_request_body(&fx.tasks.get_task(&task.id).unwrap())
        .unwrap();

    assert_eq!(body, "Adds a CSV export.\n\nCloses #3");
}

#[test]
fn opens_the_issue_with_the_pull_request_in_a_pipeline_without_a_spec_stage() {
    let fx = Fx::new();
    let (linker, _) = linker(&fx);
    opens_issues(&fx, "on-spec-approved");
    legacy_stages(&fx);
    let task = create(
        &fx,
        json!({ "title": "Export", "description": "Adds a CSV export." }),
    );

    let body = linker.pull_request_body(&task).unwrap();

    assert_eq!(body, "Adds a CSV export.\n\nCloses #12");
}

#[test]
fn opens_the_issue_with_the_pull_request_not_on_spec_approval_for_on_pr() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    opens_issues(&fx, "on-pr");
    let task = at_spec_review(&fx);
    approve_spec(&fx, &task);
    linker.settle();
    assert!(calls.lock().unwrap().is_empty());

    fx.tasks
        .update_task(
            &task.id,
            Some(&json!({ "description": "Adds a CSV export." })),
        )
        .unwrap();
    let body = |linker: &IssueLinker| {
        linker
            .pull_request_body(&fx.tasks.get_task(&task.id).unwrap())
            .unwrap()
    };
    assert_eq!(body(&linker), "Adds a CSV export.\n\nCloses #12");
    assert_eq!(calls.lock().unwrap().len(), 1);
    assert_eq!(body(&linker), "Adds a CSV export.\n\nCloses #12");
    assert_eq!(calls.lock().unwrap().len(), 1);
}

#[test]
fn opens_no_issue_for_never_on_spec_approval_or_with_the_pull_request() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    opens_issues(&fx, "never");
    legacy_stages(&fx);
    let task = create(
        &fx,
        json!({ "title": "Export", "description": "Adds a CSV export." }),
    );

    assert_eq!(
        linker.pull_request_body(&task).unwrap(),
        "Adds a CSV export."
    );
    assert!(calls.lock().unwrap().is_empty());
}

#[test]
fn writes_the_task_notes_in_the_body_when_there_is_no_spec() {
    let fx = Fx::new();
    let (linker, calls) = linker(&fx);
    let task = create(
        &fx,
        json!({ "title": "Export", "description": "Adds a CSV export." }),
    );

    linker.create(&task.id).unwrap();

    assert_eq!(
        calls.lock().unwrap()[0].args[5],
        format!("Adds a CSV export.\n\nMade by Natamux for {}.", task.id)
    );
}

#[test]
fn finds_the_issue_address_and_writes_the_closing_line() {
    assert_eq!(
        issue_url("Creating issue\nhttps://github.com/o/r/issues/9\n").as_deref(),
        Some("https://github.com/o/r/issues/9")
    );
    let github = IssueLink {
        provider: "github".into(),
        key: "#9".into(),
        url: "https://github.com/o/r/issues/9".into(),
    };
    assert_eq!(closing_line(&github), "Closes #9");
    let jira = IssueLink {
        provider: "jira".into(),
        key: "ABC-1".into(),
        url: "https://jira.example.com/ABC-1".into(),
    };
    assert_eq!(
        closing_line(&jira),
        "Tracked in ABC-1: https://jira.example.com/ABC-1"
    );
}
