#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::files::file_drafts::quit_question;

fn pair(task: &str, path: &str) -> (String, String) {
    (task.to_string(), path.to_string())
}

#[test]
fn a_plain_quit_with_no_drafts_asks_the_plain_question() {
    let text = quit_text(None, true).expect("a question");
    assert_eq!(text.title.as_ref(), "Quit Natamux?");
    assert_eq!(text.description.as_ref(), PLAIN_QUIT);
    assert!(text.rows.is_empty());
}

#[test]
fn a_restart_or_tool_run_with_no_drafts_asks_nothing() {
    assert_eq!(quit_text(None, false), None);
}

#[test]
fn drafts_ask_the_drafts_question_also_when_not_plain() {
    let drafts = quit_question(&[pair("T-0001", "src/a.rs")], &[]);
    for plain in [true, false] {
        let text = quit_text(drafts.clone(), plain).expect("a question");
        assert_eq!(text.title.as_ref(), "Quit and drop your changes?");
        let description = "These edits are not saved. Quitting drops them.";
        assert_eq!(text.description.as_ref(), description);
        assert_eq!(text.rows, vec![("T-0001".into(), "src/a.rs".into())]);
    }
}
