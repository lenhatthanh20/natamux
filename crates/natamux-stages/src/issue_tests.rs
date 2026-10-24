//! Ports `packages/daemon/src/github/__tests__/issue-ref.test.ts`.

use super::*;

const ISSUE: &str = r#"{
  "number": 7,
  "title": "Export tasks as CSV",
  "body": "We need a CSV export.",
  "url": "https://github.com/o/r/issues/7",
  "labels": [{"name": "feature"}],
  "comments": [{"author": {"login": "sam"}, "body": "Dates in UTC, please."}]
}"#;

#[test]
fn reads_the_issue_number_in_each_form() {
    let cases: [(&str, Option<u64>); 8] = [
        ("#42", Some(42)),
        ("42", Some(42)),
        ("https://github.com/o/r/issues/42", Some(42)),
        (
            "https://github.com/o/r/issues/42#issuecomment-123",
            Some(42),
        ),
        ("o/r#42", Some(42)),
        ("ABC-42", None),
        ("o/r42", None),
        ("", None),
    ];
    for (text, number) in cases {
        assert_eq!(issue_number(text), number, "{text:?}");
    }
}

#[test]
fn reads_the_repository_of_an_issue_address() {
    let repo = |number, slug: &str, repo: &str| IssueRef {
        number,
        slug: Some(slug.into()),
        repo: Some(repo.into()),
    };
    assert_eq!(
        parse_issue_ref("https://github.com/torvalds/linux/issues/1"),
        Some(repo(1, "torvalds/linux", "torvalds/linux"))
    );
    assert_eq!(
        parse_issue_ref("https://ghe.example.com/o/r/issues/5"),
        Some(repo(5, "o/r", "ghe.example.com/o/r"))
    );
    assert_eq!(
        parse_issue_ref("#5"),
        Some(IssueRef {
            number: 5,
            slug: None,
            repo: None
        })
    );
}

#[test]
fn writes_an_issue_as_markdown() {
    let text = issue_context(&GhIssue::from_json(ISSUE).unwrap());
    assert!(text.contains("# Export tasks as CSV"));
    assert!(text.contains("Labels: feature"));
    assert!(text.contains("### sam"));
}

// Cases of my own.

#[test]
fn refuses_addresses_the_regex_refuses() {
    for text in [
        "https://github.com/o/r/issues/",
        "https://github.com/o/r/pull/3",
        "https://github.com/o/issues/3",
        "https://github.com/o/r/issues/3/x",
        "ftp://github.com/o/r/issues/3",
        "o/r#",
    ] {
        assert_eq!(parse_issue_ref(text), None, "{text:?}");
    }
    assert_eq!(
        issue_number(" https://github.com/o/r/issues/3/?a=1 "),
        Some(3)
    );
}

#[test]
fn writes_the_whole_context_with_comments_and_defaults() {
    let issue = GhIssue::from_json(
        r#"{"number": 3, "comments": [{"body": " hi ", "createdAt": "2026-01-01"}]}"#,
    )
    .unwrap();
    assert_eq!(
        issue_context(&issue),
        "# Issue #3\n\nSource: \n\n(No description.)\n\n## Comments\n\n### someone, 2026-01-01\n\nhi\n"
    );
    assert_eq!(
        issue_context(&GhIssue::from_json(ISSUE).unwrap()),
        "# Export tasks as CSV\n\nSource: https://github.com/o/r/issues/7\nLabels: feature\n\n\
         We need a CSV export.\n\n## Comments\n\n### sam\n\nDates in UTC, please.\n"
    );
}
