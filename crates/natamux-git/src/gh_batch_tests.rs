use serde_json::json;

use super::tests::{calls_of, fails, list_of, says, status, stub, words};
use super::*;

#[test]
fn asks_gh_once_for_the_newest_pull_requests_of_the_repository_by_branch() {
    let (gh, calls) = stub(says(&list_of(&[
        json!({ "url": "https://github.com/o/r/pull/9", "state": "OPEN", "headRefName": "a" }),
        json!({ "url": "https://github.com/o/r/pull/8", "state": "MERGED", "headRefName": "b" }),
        json!({ "url": "https://github.com/o/r/pull/7", "state": "MERGED", "headRefName": "a" }),
    ])));

    let found = find_pull_requests(Path::new("/w"), &gh).unwrap();

    let expected: HashMap<String, String> = [
        ("a", "https://github.com/o/r/pull/9"),
        ("b", "https://github.com/o/r/pull/8"),
    ]
    .map(|(branch, url)| (branch.to_string(), url.to_string()))
    .into();
    assert_eq!(found, expected);
    let lines: Vec<String> = calls_of(&calls).iter().map(|args| args.join(" ")).collect();
    assert_eq!(
        lines,
        ["pr list --state all --json url,state,headRefName --limit 100"]
    );
}

#[test]
fn leaves_out_a_branch_whose_newest_pull_request_is_closed() {
    let (gh, _) = stub(says(&list_of(&[
        json!({ "url": "https://github.com/o/r/pull/9", "state": "CLOSED", "headRefName": "a" }),
        json!({ "url": "https://github.com/o/r/pull/7", "state": "MERGED", "headRefName": "a" }),
    ])));
    assert!(find_pull_requests(Path::new("/w"), &gh).unwrap().is_empty());
}

#[test]
fn fails_with_a_github_error_on_an_answer_that_is_not_a_json_list() {
    for answer in ["not json", "{}"] {
        let (gh, _) = stub(says(answer));
        let error = find_pull_requests(Path::new("/w"), &gh).unwrap_err();
        assert!(
            error
                .message
                .starts_with("GitHub CLI failed: could not read")
        );
    }
}

// A pull request as `gh api graphql` gives it, with the checks of its last commit.
fn pull_request_node(
    state: &str,
    is_draft: bool,
    checks: Option<serde_json::Value>,
) -> serde_json::Value {
    let rollup = checks.map(|nodes| json!({ "contexts": { "nodes": nodes } }));
    json!({ "state": state, "isDraft": is_draft,
        "commits": { "nodes": [{ "commit": { "statusCheckRollup": rollup } }] } })
}

#[test]
fn reads_a_pull_request_address() {
    let github = PullRequestRef {
        host: "github.com".into(),
        owner: "o".into(),
        name: "r".into(),
        number: 12,
    };
    let enterprise = PullRequestRef {
        host: "ghe.example.com".into(),
        owner: "team".into(),
        name: "app".into(),
        number: 3,
    };
    let cases = [
        ("https://github.com/o/r/pull/12", Some(github)),
        ("https://ghe.example.com/team/app/pull/3", Some(enterprise)),
        ("https://github.com/o/r/issues/12", None),
        ("not an address", None),
    ];
    for (url, expected) in cases {
        assert_eq!(parse_pull_request_ref(url), expected, "{url}");
    }
}

#[test]
fn maps_each_pull_request_as_the_view_does() {
    let cases = [
        (
            pull_request_node("OPEN", false, Some(json!([]))),
            json!({ "state": "OPEN", "isDraft": false, "statusCheckRollup": [] }),
        ),
        (
            pull_request_node("OPEN", true, None),
            json!({ "state": "OPEN", "isDraft": true }),
        ),
        (
            pull_request_node("MERGED", false, Some(json!([{ "state": "SUCCESS" }]))),
            json!({ "state": "MERGED", "isDraft": false, "statusCheckRollup": [{ "state": "SUCCESS" }] }),
        ),
        (
            pull_request_node(
                "OPEN",
                false,
                Some(json!([{ "status": "IN_PROGRESS", "conclusion": null }])),
            ),
            json!({ "state": "OPEN", "isDraft": false, "statusCheckRollup": [{ "status": "IN_PROGRESS" }] }),
        ),
        (
            pull_request_node(
                "OPEN",
                false,
                Some(json!([{ "status": "COMPLETED", "conclusion": "FAILURE" }])),
            ),
            json!({ "state": "OPEN", "isDraft": false,
                "statusCheckRollup": [{ "status": "COMPLETED", "conclusion": "FAILURE" }] }),
        ),
        (
            json!({ "state": "CLOSED", "isDraft": false, "commits": { "nodes": [] } }),
            json!({ "state": "CLOSED" }),
        ),
    ];
    let mut repository = serde_json::Map::new();
    let mut numbers = Vec::new();
    for (at, (node, _)) in cases.iter().enumerate() {
        let number = at as u64 + 1;
        repository.insert(format!("pr{number}"), node.clone());
        numbers.push(number);
    }
    let output = json!({ "data": { "repository": repository } }).to_string();

    let statuses = parse_pull_request_batch(&output, &numbers).unwrap();

    for (at, (_, view)) in cases.iter().enumerate() {
        let expected = parse_pull_request_view(&view.to_string()).unwrap();
        assert_eq!(statuses.get(&(at as u64 + 1)), Some(&expected));
    }
}

#[test]
fn leaves_out_a_number_that_github_did_not_find() {
    let repository =
        json!({ "pr1": pull_request_node("OPEN", false, Some(json!([]))), "pr2": null });
    let output = json!({ "data": { "repository": repository } }).to_string();
    let statuses = parse_pull_request_batch(&output, &[1, 2]).unwrap();
    assert_eq!(statuses.keys().copied().collect::<Vec<_>>(), [1]);
}

#[test]
fn fails_without_a_repository_in_the_answer() {
    let output = json!({ "data": { "repository": null } }).to_string();
    assert!(parse_pull_request_batch(&output, &[1]).is_err());
}

fn repo() -> RepoRef {
    RepoRef {
        host: "github.com".into(),
        owner: "o".into(),
        name: "r".into(),
    }
}

fn batch_answer(repository: serde_json::Value) -> String {
    json!({ "data": { "repository": repository } }).to_string()
}

#[test]
fn asks_for_all_pull_requests_of_a_repository_in_one_graphql_call() {
    let answer = batch_answer(
        json!({ "pr1": pull_request_node("OPEN", false, Some(json!([]))), "pr7": null }),
    );
    let (gh, calls) = stub(says(&answer));

    let statuses = read_pull_request_statuses(Path::new("/repo"), &repo(), &[1, 7], &gh).unwrap();

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses.get(&1), Some(&status("open", "none")));
    let calls = calls_of(&calls);
    assert_eq!(calls.len(), 1);
    let args = &calls[0];
    assert_eq!(args[..2], ["api", "graphql"]);
    assert!(args.contains(&"owner=o".to_string()));
    assert!(args.contains(&"name=r".to_string()));
    assert!(!args.contains(&"--hostname".to_string()));
    let query = args.iter().find(|arg| arg.starts_with("query=")).unwrap();
    assert!(query.contains("pr1: pullRequest(number: 1)"));
    assert!(query.contains("pr7: pullRequest(number: 7)"));
    let fields = PULL_REQUEST_FIELDS.join(" ");
    let head =
        "query=query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {";
    let expected = format!(
        "{head} pr1: pullRequest(number: 1) {{ {fields} }} pr7: pullRequest(number: 7) {{ {fields} }} }} }}"
    );
    assert_eq!(*query, expected);
}

#[test]
fn names_the_host_of_github_enterprise() {
    let answer = batch_answer(json!({ "pr1": pull_request_node("OPEN", false, Some(json!([]))) }));
    let (gh, calls) = stub(says(&answer));
    let enterprise = RepoRef {
        host: "ghe.example.com".into(),
        ..repo()
    };

    read_pull_request_statuses(Path::new("/repo"), &enterprise, &[1], &gh).unwrap();

    assert_eq!(calls_of(&calls)[0][2..4], ["--hostname", "ghe.example.com"]);
}

#[test]
fn reads_the_answer_gh_printed_before_it_failed_for_a_number_it_did_not_find() {
    let printed = batch_answer(
        json!({ "pr1": pull_request_node("MERGED", false, Some(json!([]))), "pr2": null }),
    );
    let (gh, _) = stub(fails(GitHubError::with_output(
        "Could not resolve to a PullRequest",
        &printed,
    )));

    let statuses = read_pull_request_statuses(Path::new("/repo"), &repo(), &[1, 2], &gh).unwrap();

    assert_eq!(statuses.len(), 1);
    assert_eq!(statuses.get(&1), Some(&status("merged", "none")));
}

#[test]
fn passes_on_a_failure_without_an_answer() {
    let (gh, _) = stub(fails(GitHubError::new("not logged in")));
    let error = read_pull_request_statuses(Path::new("/repo"), &repo(), &[1], &gh).unwrap_err();
    assert!(error.message.contains("not logged in"));
}

#[test]
fn fails_with_a_github_error_on_a_batch_answer_that_is_not_json() {
    let (gh, _) = stub(says("not json"));
    let error = read_pull_request_statuses(Path::new("/repo"), &repo(), &[1], &gh).unwrap_err();
    assert_eq!(
        error.message,
        "GitHub CLI failed: could not read the pull requests: not json"
    );
}

#[test]
fn says_gh_timed_out_when_the_time_limit_killed_it() {
    let killed = Captured {
        code: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: true,
        overflow: false,
        not_found: false,
    };
    let error = gh_error_of(&killed, &words(&["pr", "view"]));
    assert_eq!(
        error.message,
        "GitHub CLI failed: gh did not finish in 60 seconds"
    );
}

#[test]
fn says_gh_printed_too_much_when_the_output_limit_killed_it() {
    let killed = Captured {
        code: None,
        stdout: String::new(),
        stderr: String::new(),
        timed_out: false,
        overflow: true,
        not_found: false,
    };
    let error = gh_error_of(&killed, &words(&["pr", "view"]));
    assert_eq!(
        error.message,
        "GitHub CLI failed: gh printed more than 10 MiB of output"
    );
}

#[test]
fn keeps_the_stderr_and_stdout_of_a_gh_that_failed_by_itself() {
    let failed = Captured {
        code: Some(1),
        stdout: "{\"data\":{}}".into(),
        stderr: "not logged in\n".into(),
        timed_out: false,
        overflow: false,
        not_found: false,
    };
    let error = gh_error_of(&failed, &words(&["pr", "view"]));
    assert_eq!(error.message, "GitHub CLI failed: not logged in");
    assert_eq!(error.output, "{\"data\":{}}");
}
