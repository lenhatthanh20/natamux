//! `describe('attachments')` of `stage-runner.test.ts`.

use std::fs;
use std::path::Path;

use natamux_proto::tasks::task::{Task, TaskState};
use natamux_store::attachments::AttachmentFile;
use serde_json::{Value, json};

use crate::context::{context_of, report, set_sources, skill_source, write_working};
use crate::{Fixture, run_git};

fn files() -> Vec<AttachmentFile> {
    [("login error.png", "image"), ("spec.docx", "word")]
        .iter()
        .map(|(name, text)| AttachmentFile {
            name: (*name).into(),
            bytes: text.as_bytes().to_vec(),
        })
        .collect()
}

const LISTED: [&str; 2] = [
    "- `.natamux-task/attachments/login-error.png` (5 B)",
    "- `.natamux-task/attachments/spec.docx` (4 B)",
];

fn created_with_files(f: &Fixture, input: Value) -> Task {
    let task = f.tasks.create_task(Some(&input), &files(), true).unwrap();
    f.settle();
    f.task(&task.id)
}

fn workspace_file(task: &Task, name: &str) -> String {
    fs::read_to_string(Path::new(task.workspace.as_deref().unwrap()).join(name)).unwrap()
}

#[test]
fn lists_the_attachments_in_the_context_of_a_typed_task() {
    let f = Fixture::new();
    let task = created_with_files(
        &f,
        json!({ "title": "Export tasks", "description": "As CSV." }),
    );

    assert_eq!(
        (task.stage.as_str(), task.state),
        ("spec", TaskState::Ready)
    );
    let context = context_of(&f, &task.id);
    assert!(context.starts_with("# Export tasks\n\nAs CSV.\n\n## Attachments\n"));
    for line in LISTED {
        assert!(context.contains(line));
    }
}

#[test]
fn lists_the_attachments_in_the_context_read_from_a_github_issue() {
    let f = Fixture::new();
    let task = created_with_files(
        &f,
        json!({ "title": "Export", "context": { "source": "github-issue", "ref": "#7" } }),
    );

    let context = context_of(&f, &task.id);
    assert!(context.contains("Dates in UTC, please."));
    assert!(context.contains("## Attachments"));
    assert!(context.contains(LISTED[0]));
}

#[test]
fn puts_the_attachments_into_the_worktree_where_the_agent_finds_them() {
    let f = Fixture::new();
    let made = created_with_files(&f, json!({ "title": "Export" }));
    let task = f.with_workspace(&made.id);

    assert_eq!(
        workspace_file(&task, ".natamux-task/attachments/login-error.png"),
        "image"
    );
    assert_eq!(
        workspace_file(&task, ".natamux-task/attachments/spec.docx"),
        "word"
    );
    assert_eq!(
        run_git(
            Path::new(task.workspace.as_deref().unwrap()),
            &["status", "--porcelain"]
        ),
        ""
    );
}

fn jira_pick() -> Value {
    json!({
        "title": "Export",
        "agent": "claude-code",
        "context": { "source": "skill", "name": "Jira", "ref": "ABC-1" },
    })
}

#[test]
fn names_the_attachments_to_a_context_agent_and_adds_them_when_it_leaves_them_out() {
    let f = Fixture::new();
    set_sources(&f, vec![skill_source("Jira", "jira-ticket")]);
    let waiting = created_with_files(&f, jira_pick());
    let task = f.with_workspace(&waiting.id);
    let script = f.launch_script(&task.id, "1");
    assert!(script.contains(".natamux-task/attachments/login-error.png (5 B)"));
    assert!(script.contains("list them under an Attachments heading"));

    write_working(&task, "context.md", "# ABC-1\n\nExport as CSV.\n");
    report(&f, &task.id, "1");

    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("context", TaskState::Review)
    );
    let context = workspace_file(&task, ".natamux-task/context.md");
    assert!(context.starts_with("# ABC-1\n\nExport as CSV.\n\n## Attachments\n"));
    assert!(context.contains(LISTED[1]));
}

#[test]
fn keeps_the_attachments_section_an_agent_wrote_itself() {
    let f = Fixture::new();
    set_sources(&f, vec![skill_source("Jira", "jira-ticket")]);
    let waiting = created_with_files(&f, jira_pick());
    let task = f.with_workspace(&waiting.id);
    let written = "# ABC-1\n\n## Attachments\n\n- the screenshot and the spec\n";
    write_working(&task, "context.md", written);
    report(&f, &task.id, "1");

    assert_eq!(workspace_file(&task, ".natamux-task/context.md"), written);
}
