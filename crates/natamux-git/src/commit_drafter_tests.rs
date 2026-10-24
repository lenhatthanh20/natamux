use std::cell::RefCell;

use natamux_core::js::utf16_len;
use natamux_proto::workspaces::changes::ChangeStatus;

use super::*;

fn input() -> DraftInput {
    DraftInput {
        agent: "claude-code".into(),
        program: "claude".into(),
        cwd: PathBuf::from("/work"),
        title: "Fix the parser".into(),
        changes: vec![ChangeEntry {
            path: "src/a.ts".into(),
            status: ChangeStatus::Modified,
            original_path: None,
            added: None,
            removed: None,
        }],
        diff: "src/a.ts | 2 +-\n+fixed".into(),
        timeout_ms: None,
    }
}

fn simple_draft(reason: &str) -> CommitMessageDraft {
    CommitMessageDraft {
        message: "Update src/a.ts".into(),
        source: DraftSource::Draft,
        agent: "claude-code".into(),
        reason: Some(reason.into()),
    }
}

#[test]
fn holds_the_rules_the_task_title_and_the_diff() {
    let prompt = commit_prompt("Fix the parser", "the diff");
    assert!(prompt.contains("Output the message only."));
    assert!(prompt.contains("at most 72 characters"));
    assert!(prompt.contains("No Co-Authored-By"));
    assert!(prompt.contains("Task: Fix the parser"));
    assert!(prompt.ends_with("the diff"));
}

#[test]
fn puts_the_rules_first_then_the_title_then_the_diff_with_its_cut_mark() {
    let diff = " a.ts | 2 +-\n\ndiff --git a/a.ts b/a.ts\n[diff cut: 12 more characters]";
    let prompt = commit_prompt("", diff);
    assert!(prompt.starts_with("Write one git commit message"));
    let at = |part: &str| prompt.find(part).unwrap();
    assert!(at("Do not run commands") < at("Task: "));
    assert!(at("Task: ") < at(diff));
    assert!(prompt.ends_with("[diff cut: 12 more characters]"));
    for rule in [
        "imperative",
        "no period",
        "No Markdown",
        "Do not run commands",
    ] {
        assert!(prompt.contains(rule), "{rule}");
    }
}

#[test]
fn turns_what_the_agent_printed_into_a_message() {
    let cases = [
        ("  Fix the parser \n", "Fix the parser"),
        ("```\nFix the parser\n```", "Fix the parser"),
        (
            "```text\nFix the parser\n\nIt broke on tabs.\n```",
            "Fix the parser\n\nIt broke on tabs.",
        ),
        ("\"Fix the parser\"", "Fix the parser"),
        ("\u{1b}[1mFix the parser\u{1b}[0m", "Fix the parser"),
        ("Fix the parser\r\n\r\nBody", "Fix the parser\n\nBody"),
        (
            "Fix the parser\n\nCo-Authored-By: Claude <noreply@anthropic.com>",
            "Fix the parser",
        ),
        (
            "Fix the parser\n\n🤖 Generated with [Claude Code](https://claude.com)",
            "Fix the parser",
        ),
        ("Co-authored-by: x", ""),
        ("   ", ""),
        ("'Add login'", "Add login"),
        ("  Add login  \n\n", "Add login"),
        ("Add login\n\ngenerated WITH x", "Add login"),
        ("Add \"login\" form", "Add \"login\" form"),
        (
            "Add login\n\n```\ncode\n```\nmore",
            "Add login\n\n```\ncode\n```\nmore",
        ),
        (
            "Add login\n\nWhy.\n\nCo-Authored-By: X\n\n",
            "Add login\n\nWhy.",
        ),
        (
            "Built with tools that generated with care",
            "Built with tools that generated with care",
        ),
    ];
    for (output, message) in cases {
        assert_eq!(clean_commit_message(output), message, "{output:?}");
    }
}

#[test]
fn cuts_a_long_first_line_and_a_long_message() {
    let message =
        clean_commit_message(&format!("{}\n\n{}", "word ".repeat(30), "b".repeat(20_000)));
    let first = message.split('\n').next().unwrap();
    assert!(utf16_len(first) <= 72);
    assert!(utf16_len(&message) <= 10_000);
}

#[test]
fn cuts_a_first_line_of_90_characters_at_the_last_space_before_72_and_keeps_the_body() {
    let first = "abcd ".repeat(18).trim().to_string();
    let message = clean_commit_message(&format!("{first}\n\nThe body."));
    let lines: Vec<&str> = message.split('\n').collect();
    assert!(first.starts_with(lines[0]));
    assert!(utf16_len(lines[0]) <= 72);
    assert!(lines[0].ends_with("abcd"));
    assert_eq!(lines[1..], ["", "The body."]);
}

#[test]
fn keeps_a_first_line_of_exactly_72_and_cuts_one_without_spaces_at_72() {
    assert_eq!(clean_commit_message(&"a".repeat(72)), "a".repeat(72));
    assert_eq!(clean_commit_message(&"a".repeat(90)), "a".repeat(72));
}

#[test]
fn cuts_a_message_of_15000_characters_to_10000() {
    let message = clean_commit_message(&format!("Add login\n\n{}", "b".repeat(15_000)));
    assert_eq!(utf16_len(&message), 10_000);
}

#[test]
fn asks_the_agent_once_in_the_workspace_with_the_prompt() {
    let input = input();
    let runs = RefCell::new(Vec::new());
    let draft = draft_commit_message(&input, &|run| {
        runs.borrow_mut().push(run);
        Ok("Fix the parser\n".into())
    });

    assert_eq!(
        draft,
        CommitMessageDraft {
            message: "Fix the parser".into(),
            source: DraftSource::Agent,
            agent: "claude-code".into(),
            reason: None,
        }
    );
    assert_eq!(
        *runs.borrow(),
        [AgentRun {
            agent: "claude-code".into(),
            program: "claude".into(),
            prompt: commit_prompt("Fix the parser", &input.diff),
            cwd: PathBuf::from("/work"),
            timeout_ms: 60_000,
        }]
    );
}

#[test]
fn gives_the_simple_draft_with_the_reason_when_the_agent_fails() {
    let draft = draft_commit_message(&input(), &|_| Err("claude timed out after 60 s".into()));
    assert_eq!(draft, simple_draft("claude timed out after 60 s"));
}

#[test]
fn gives_the_simple_draft_when_the_agent_prints_only_nothing_of_use() {
    let outputs = [
        "",
        "  \n\t ",
        "\u{1b}[0m\u{1b}[2K",
        "```\n\n```",
        "Co-Authored-By: Claude <noreply@anthropic.com>",
    ];
    for output in outputs {
        let draft = draft_commit_message(&input(), &|_| Ok(output.to_string()));
        assert_eq!(
            draft,
            simple_draft("claude printed no message"),
            "{output:?}"
        );
    }
}

#[test]
fn waits_60_seconds_by_default() {
    assert_eq!(DRAFT_TIMEOUT_MS, 60_000);
}
