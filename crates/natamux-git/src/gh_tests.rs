use std::sync::Mutex;

use serde_json::json;

use super::*;
use crate::test_support::TempDir;

#[test]
fn says_when_gh_is_missing_or_fails() {
    // gh may or may not be installed here; either way the answer is an error text or output.
    let dir = TempDir::new("gh");
    let answer = run_gh(dir.path(), &["no-such-command-xyz".to_string()]);
    if let Err(error) = answer {
        assert!(error.message.starts_with("GitHub CLI failed: "), "{error}");
    }
}

#[cfg(unix)]
#[test]
fn reports_a_gh_that_does_not_finish() {
    let dir = TempDir::new("gh");
    let bin = dir.path().join("bin");
    std::fs::create_dir(&bin).unwrap();
    let script = bin.join("gh");
    std::fs::write(&script, "#!/bin/sh\nsleep 30\n").unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut command = Command::new(&script);
    command.current_dir(dir.path());
    let ran = capture(command, Duration::from_millis(200), 1024);
    assert!(ran.killed());
}

type Calls = Arc<Mutex<Vec<Vec<String>>>>;

// A `RunGh` that answers with `answer` and keeps the arguments of each call.
pub(super) fn stub(
    answer: impl Fn() -> Result<String, GitHubError> + Send + Sync + 'static,
) -> (RunGh, Calls) {
    let calls: Calls = Arc::default();
    let seen = Arc::clone(&calls);
    let gh: RunGh = Arc::new(move |_: &Path, args: &[String]| {
        seen.lock().unwrap().push(args.to_vec());
        answer()
    });
    (gh, calls)
}

pub(super) fn says(text: &str) -> impl Fn() -> Result<String, GitHubError> + Send + Sync + 'static {
    let text = text.to_string();
    move || Ok(text.clone())
}

pub(super) fn fails(
    error: GitHubError,
) -> impl Fn() -> Result<String, GitHubError> + Send + Sync + 'static {
    move || Err(error.clone())
}

pub(super) fn calls_of(calls: &Calls) -> Vec<Vec<String>> {
    calls.lock().unwrap().clone()
}

pub(super) fn words(args: &[&str]) -> Vec<String> {
    args.iter().map(|arg| arg.to_string()).collect()
}

pub(super) fn status(state: &str, checks: &str) -> PullRequestStatus {
    PullRequestStatus {
        state: state.into(),
        checks: checks.into(),
    }
}

fn create(gh: &RunGh, base: Option<&str>) -> Result<String, GitHubError> {
    create_pull_request(
        Path::new("/w"),
        "natamux/t-0001",
        "Fix it",
        "Body",
        base,
        gh,
    )
}

#[test]
fn finds_the_address_in_the_output() {
    let cases = [
        (
            "https://github.com/o/r/pull/12\n",
            Some("https://github.com/o/r/pull/12"),
        ),
        (
            "Creating pull request\nhttps://ghe.example.com/o/r/pull/3",
            Some("https://ghe.example.com/o/r/pull/3"),
        ),
        ("nothing here", None),
    ];
    for (output, url) in cases {
        assert_eq!(pull_request_url(output).as_deref(), url, "{output:?}");
    }
}

#[test]
fn asks_gh_for_a_pull_request_from_the_branch_and_gives_its_address() {
    let (gh, calls) = stub(says("https://github.com/o/r/pull/12\n"));

    assert_eq!(
        create(&gh, None),
        Ok("https://github.com/o/r/pull/12".to_string())
    );
    assert_eq!(
        calls_of(&calls),
        [words(&[
            "pr",
            "create",
            "--head",
            "natamux/t-0001",
            "--title",
            "Fix it",
            "--body",
            "Body"
        ])]
    );
}

#[test]
fn asks_for_the_base_branch_when_one_is_given() {
    let (gh, calls) = stub(says("https://github.com/o/r/pull/13\n"));
    create(&gh, Some("natamux/t-0000")).unwrap();
    assert_eq!(
        calls_of(&calls),
        [words(&[
            "pr",
            "create",
            "--head",
            "natamux/t-0001",
            "--base",
            "natamux/t-0000",
            "--title",
            "Fix it",
            "--body",
            "Body"
        ])]
    );
}

#[test]
fn gives_the_address_of_the_pull_request_that_exists_already() {
    let (gh, _) = stub(fails(GitHubError::new(
        "a pull request for branch \"natamux/t-0001\" into branch \"main\" already exists:\n\
         https://github.com/o/r/pull/9",
    )));
    assert_eq!(
        create(&gh, None),
        Ok("https://github.com/o/r/pull/9".to_string())
    );
}

#[test]
fn passes_on_any_other_failure() {
    let (gh, _) = stub(fails(GitHubError::new("not logged in")));
    let error = create(&gh, None).unwrap_err();
    assert!(error.message.contains("not logged in"));
}

#[test]
fn fails_when_gh_answers_without_an_address() {
    let (gh, _) = stub(says("ok"));
    let error = create(&gh, None).unwrap_err();
    assert_eq!(
        error.message,
        "GitHub CLI failed: no pull request address in the answer: ok"
    );
}

fn item(status: Option<&str>, conclusion: Option<&str>, state: Option<&str>) -> CheckItem {
    CheckItem {
        status: status.map(str::to_string),
        conclusion: conclusion.map(str::to_string),
        state: state.map(str::to_string),
    }
}

#[test]
fn reads_checks_as_a_summary() {
    let done = |conclusion| item(Some("COMPLETED"), Some(conclusion), None);
    let cases = [
        (vec![], "none"),
        (
            vec![done("SUCCESS"), item(None, None, Some("SUCCESS"))],
            "passing",
        ),
        (vec![done("SKIPPED")], "passing"),
        (
            vec![item(Some("IN_PROGRESS"), None, None), done("SUCCESS")],
            "pending",
        ),
        (vec![item(None, None, Some("PENDING"))], "pending"),
        (
            vec![item(Some("IN_PROGRESS"), None, None), done("FAILURE")],
            "failing",
        ),
        (vec![item(None, None, Some("ERROR"))], "failing"),
        (vec![done("TIMED_OUT")], "failing"),
    ];
    for (items, summary) in cases {
        assert_eq!(summarize_checks(&items), summary, "{items:?}");
    }
}

#[test]
fn reads_a_pull_request_view() {
    let cases = [
        (
            json!({ "state": "OPEN", "isDraft": false, "statusCheckRollup": [] }),
            status("open", "none"),
        ),
        (
            json!({ "state": "OPEN", "isDraft": true, "statusCheckRollup": null }),
            status("draft", "none"),
        ),
        (
            json!({ "state": "MERGED", "isDraft": false, "statusCheckRollup": [{ "state": "SUCCESS" }] }),
            status("merged", "passing"),
        ),
        (json!({ "state": "CLOSED" }), status("closed", "none")),
    ];
    for (data, expected) in cases {
        assert_eq!(parse_pull_request_view(&data.to_string()), Ok(expected));
    }
}

#[test]
fn asks_gh_for_the_state_and_the_checks_of_the_pull_request() {
    let answer = json!({ "state": "OPEN", "isDraft": false, "statusCheckRollup": [] });
    let (gh, calls) = stub(says(&answer.to_string()));
    let url = "https://github.com/o/r/pull/1";

    assert_eq!(
        read_pull_request_status(Path::new("/repo"), url, &gh),
        Ok(status("open", "none"))
    );
    assert_eq!(
        calls_of(&calls),
        [words(&[
            "pr",
            "view",
            url,
            "--json",
            "state,isDraft,statusCheckRollup"
        ])]
    );
}

#[test]
fn fails_with_a_github_error_on_an_answer_that_is_not_json() {
    let (gh, _) = stub(says("not json"));
    let url = "https://github.com/o/r/pull/1";
    let error = read_pull_request_status(Path::new("/repo"), url, &gh).unwrap_err();
    assert_eq!(
        error.message,
        "GitHub CLI failed: could not read the pull request: not json"
    );
}

pub(super) fn list_of(items: &[serde_json::Value]) -> String {
    serde_json::Value::Array(items.to_vec()).to_string()
}

#[test]
fn asks_gh_for_the_newest_pull_request_from_the_branch() {
    let (gh, calls) = stub(says(&list_of(&[
        json!({ "url": "https://github.com/o/r/pull/7", "state": "OPEN" }),
    ])));

    let found = find_pull_request(Path::new("/w"), "natamux/t-0001", &gh);

    assert_eq!(found, Ok(Some("https://github.com/o/r/pull/7".to_string())));
    let lines: Vec<String> = calls_of(&calls).iter().map(|args| args.join(" ")).collect();
    assert_eq!(
        lines,
        ["pr list --head natamux/t-0001 --state all --json url,state --limit 1"]
    );
}

#[test]
fn finds_a_merged_pull_request() {
    let (gh, _) = stub(says(&list_of(&[
        json!({ "url": "https://github.com/o/r/pull/8", "state": "MERGED" }),
    ])));
    assert_eq!(
        find_pull_request(Path::new("/w"), "b", &gh),
        Ok(Some("https://github.com/o/r/pull/8".to_string()))
    );
}

#[test]
fn finds_nothing_for_no_pull_request_or_a_closed_one() {
    let lists = [
        list_of(&[]),
        list_of(&[json!({ "url": "https://github.com/o/r/pull/9", "state": "CLOSED" })]),
    ];
    for list in lists {
        let (gh, _) = stub(says(&list));
        assert_eq!(find_pull_request(Path::new("/w"), "b", &gh), Ok(None));
    }
}

#[test]
fn fails_with_a_github_error_on_a_pull_request_list_that_is_not_json() {
    let (gh, _) = stub(says("not json"));
    assert!(find_pull_request(Path::new("/w"), "b", &gh).is_err());
}
