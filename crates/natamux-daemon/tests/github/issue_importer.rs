//! `issue-importer.test.ts`.

use natamux_daemon::github::issue_importer::{
    IssueImportError, IssueImporter, IssueImporterOptions, parse_issue_list,
};
use natamux_daemon::tasks::errors::TaskError;
use natamux_proto::tasks::task::TaskState;
use serde_json::{Value, json};

use super::{Calls, Fx, jobs, recording, repo_path};

fn issues() -> Value {
    json!([
        {
            "number": 3,
            "title": "Crash on start",
            "body": "Steps:\n1. Start",
            "url": "https://github.com/o/r/issues/3",
            "labels": [{ "name": "bug" }],
        },
        { "number": 4, "title": "", "body": "", "url": "https://github.com/o/r/issues/4", "labels": [] },
    ])
}

fn importer_with(fx: &Fx, answer: Value) -> (IssueImporter, Calls) {
    let (gh, calls) = recording(move |_| Ok(answer.to_string()));
    let importer = IssueImporter::new(IssueImporterOptions {
        tasks: fx.tasks.clone(),
        repo_path: repo_path(),
        gh,
        jobs: jobs(),
    });
    (importer, calls)
}

fn importer(fx: &Fx) -> (IssueImporter, Calls) {
    importer_with(fx, issues())
}

#[test]
fn keeps_the_fields_it_needs_and_skips_items_without_a_number_or_an_address() {
    let mut items = issues().as_array().unwrap().clone();
    items.push(json!({ "title": "broken" }));

    let found = parse_issue_list(&Value::Array(items).to_string()).unwrap();

    assert_eq!(found.len(), 2);
    assert_eq!(found[0].number, 3);
    assert_eq!(found[0].title, "Crash on start");
    assert_eq!(found[0].body, "Steps:\n1. Start");
    assert_eq!(found[0].url, "https://github.com/o/r/issues/3");
    assert_eq!(found[0].labels, ["bug"]);
    assert_eq!(found[1].number, 4);
    assert_eq!(found[1].title, "");
    assert!(found[1].labels.is_empty());
}

#[test]
fn fails_with_a_github_error_on_an_answer_that_is_not_json() {
    let error = parse_issue_list("nope").unwrap_err();

    assert_eq!(
        error.message,
        "GitHub CLI failed: could not read the issue list: nope"
    );
}

#[test]
fn stops_at_an_issue_whose_title_names_an_id_that_is_used() {
    let fx = Fx::new();
    fx.tasks
        .create_task(Some(&json!({ "title": "ABC-9: Existing" })), &[], true)
        .unwrap();
    let issue = |number: u64, title: &str| {
        json!({
            "number": number,
            "title": title,
            "body": "",
            "url": format!("https://github.com/o/r/issues/{number}"),
            "labels": [],
        })
    };
    let answer = json!([issue(5, "One"), issue(6, "ABC-9: Two"), issue(7, "Three")]);
    let (clashing, _) = importer_with(&fx, answer);

    let error = clashing.import(&[5, 6, 7], None).unwrap_err();

    assert!(matches!(error, IssueImportError::Task(TaskError::Edit(_))));
    let mut titles: Vec<String> = fx
        .tasks
        .list_tasks(None)
        .unwrap()
        .into_iter()
        .map(|task| task.title)
        .collect();
    titles.sort();
    assert_eq!(titles, ["Existing", "One"]);
}

#[test]
fn lists_the_open_issues_with_gh() {
    let fx = Fx::new();
    let (importer, calls) = importer(&fx);

    let issues = importer.list(None).unwrap();

    let numbers: Vec<_> = issues
        .iter()
        .map(|issue| (issue.number, issue.task_id.clone()))
        .collect();
    assert_eq!(numbers, [(3, None), (4, None)]);
    let calls = calls.lock().unwrap();
    assert_eq!(calls[0].cwd, "/repo");
    assert_eq!(
        calls[0].args,
        [
            "issue",
            "list",
            "--state",
            "open",
            "--limit",
            "100",
            "--json",
            "number,title,body,url,labels"
        ]
    );
}

#[test]
fn makes_a_draft_task_from_an_issue_once() {
    let fx = Fx::new();
    let (importer, _) = importer(&fx);

    let made = importer.import(&[3], None).unwrap();

    let task = &made[0];
    assert_eq!(task.title, "Crash on start");
    assert_eq!(
        task.description,
        "Steps:\n1. Start\n\nImported from https://github.com/o/r/issues/3"
    );
    assert_eq!(task.labels, ["bug"]);
    assert_eq!(task.state, TaskState::Draft);
    assert!(importer.import(&[3], None).unwrap().is_empty());
    let listed = importer.list(None).unwrap();
    let issue = listed.iter().find(|issue| issue.number == 3).unwrap();
    assert_eq!(issue.task_id.as_deref(), Some(task.id.as_str()));
}

#[test]
fn names_an_issue_without_a_title_and_skips_numbers_that_are_not_open_issues() {
    let fx = Fx::new();
    let (importer, _) = importer(&fx);

    let made = importer.import(&[4, 99], None).unwrap();

    assert_eq!(made.len(), 1);
    assert_eq!(made[0].title, "Issue #4");
    assert_eq!(
        made[0].description,
        "Imported from https://github.com/o/r/issues/4"
    );
}
