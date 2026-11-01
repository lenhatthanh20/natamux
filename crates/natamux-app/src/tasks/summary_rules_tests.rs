use natamux_proto::tasks::task::PullRequestStatus;

use super::*;

const URL: &str = "https://github.com/o/r/pull/12345";

fn task(pr: Option<&str>, status: Option<(&str, &str)>) -> Task {
    Task {
        id: "T-0001".into(),
        pr: pr.map(String::from),
        pr_status: status.map(|(state, checks)| PullRequestStatus {
            state: state.into(),
            checks: checks.into(),
        }),
        ..Task::default()
    }
}

fn usage(tokens: u64, cost_usd: Option<f64>) -> TaskUsage {
    TaskUsage {
        tokens,
        cost_usd,
        ..TaskUsage::default()
    }
}

#[test]
fn shows_the_pull_request_and_its_checks() {
    let facts = summary_facts(&task(Some(URL), Some(("open", "passing"))), None);
    assert_eq!(
        facts.pr,
        Some(PrFacts {
            url: URL.into(),
            label: "#12345".into(),
            status: Some(("open · checks pass".into(), ChecksTone::Passing)),
        })
    );
}

#[test]
fn shows_the_state_alone_without_checks() {
    let facts = summary_facts(&task(Some(URL), Some(("merged", "none"))), None);
    let status = facts.pr.and_then(|pr| pr.status);
    assert_eq!(status, Some(("merged".into(), ChecksTone::None)));
}

#[test]
fn shows_only_the_number_before_github_answers() {
    let facts = summary_facts(&task(Some(URL), None), None);
    let pr = facts.pr.expect("a pull request");
    assert_eq!(pr.label, "#12345");
    assert_eq!(pr.status, None);
}

#[test]
fn names_a_pull_request_address_without_a_number() {
    let facts = summary_facts(&task(Some("https://example.com/x"), None), None);
    assert_eq!(facts.pr.map(|pr| pr.label), Some("Pull request".into()));
}

#[test]
fn has_no_pull_request_side_without_a_pr() {
    let facts = summary_facts(&task(None, None), None);
    assert_eq!(facts.pr, None);
    assert_eq!(NO_PR, "No PR yet");
}

#[test]
fn shows_cost_and_tokens_when_priced() {
    let used = usage(2_400_000, Some(4.123));
    let facts = summary_facts(&task(None, None), Some(&used));
    assert_eq!(facts.cost.as_deref(), Some("$4.12 est."));
    assert_eq!(facts.tokens.as_deref(), Some("2.4M tok"));
}

#[test]
fn shows_tokens_alone_without_a_price() {
    let used = usage(640_000, None);
    let facts = summary_facts(&task(None, None), Some(&used));
    assert_eq!(facts.cost, None);
    assert_eq!(facts.tokens.as_deref(), Some("640K tok"));
}

#[test]
fn shows_no_usage_when_the_agent_reports_none() {
    let facts = summary_facts(&task(None, None), None);
    assert_eq!((facts.cost, facts.tokens), (None, None));
    let empty = usage(0, None);
    let facts = summary_facts(&task(None, None), Some(&empty));
    assert_eq!((facts.cost, facts.tokens), (None, None));
}
