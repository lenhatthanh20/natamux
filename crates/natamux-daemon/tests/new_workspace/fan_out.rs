//! `fan-out.test.ts`.

use natamux_daemon::workspaces::fan_out::{fan_out, try_label};
use natamux_proto::tasks::task::TaskState;
use natamux_store::attachments::{AttachmentFile, list_attachments};
use serde_json::json;

use crate::Fx;

fn create(fx: &Fx, input: serde_json::Value) -> natamux_proto::tasks::task::Task {
    fx.tasks.create_task(Some(&input), &[], true).unwrap()
}

#[test]
fn labels_the_tries_of_a_task_with_a_chosen_id_and_gives_them_default_ids() {
    let fx = Fx::without_runner();
    let source = create(&fx, json!({ "title": "Task-0005: abc-2 y" }));

    let tries = fan_out(&fx.fan_parts(), &source.id, 2, false).unwrap();

    assert_eq!(
        (source.id.as_str(), source.title.as_str()),
        ("TASK-0005", "abc-2 y")
    );
    let seen: Vec<(String, String, Vec<String>)> = tries
        .into_iter()
        .map(|task| (task.id, task.title, task.labels))
        .collect();
    let label = vec!["tries-task-0005".to_string()];
    assert_eq!(
        seen,
        [
            (
                "T-0001".to_string(),
                "abc-2 y (try 2)".to_string(),
                label.clone()
            ),
            ("T-0002".to_string(), "abc-2 y (try 3)".to_string(), label),
        ]
    );
}

#[test]
fn copies_a_task_and_labels_it_and_its_tries_alike() {
    let fx = Fx::without_runner();
    let source = create(
        &fx,
        json!({
            "title": "Speed up search",
            "description": "Under 100 ms",
            "priority": "high",
            "labels": ["perf"],
        }),
    );

    let tries = fan_out(&fx.fan_parts(), &source.id, 2, false).unwrap();

    let label = try_label(&source.id);
    assert_eq!(label, "tries-t-0001");
    let titles: Vec<&str> = tries.iter().map(|task| task.title.as_str()).collect();
    assert_eq!(
        titles,
        ["Speed up search (try 2)", "Speed up search (try 3)"]
    );
    let first = &tries[0];
    assert_eq!(first.description, "Under 100 ms");
    assert_eq!(first.priority, "high");
    assert_eq!(first.labels, ["perf".to_string(), label.clone()]);
    assert_eq!(first.state, TaskState::Draft);
    assert_eq!(first.workspace, None);
    assert_eq!(fx.task(&source.id).labels, ["perf".to_string(), label]);
    assert_eq!(fx.pty_count(), 0);
}

#[test]
fn numbers_the_tries_of_a_second_fan_out_after_the_first_ones() {
    let fx = Fx::without_runner();
    let source = create(&fx, json!({ "title": "Speed up search" }));

    fan_out(&fx.fan_parts(), &source.id, 2, false).unwrap();
    let more = fan_out(&fx.fan_parts(), &source.id, 1, false).unwrap();

    let titles: Vec<&str> = more.iter().map(|task| task.title.as_str()).collect();
    assert_eq!(titles, ["Speed up search (try 4)"]);
}

#[test]
fn gives_each_try_the_attachments_of_the_task() {
    let fx = Fx::without_runner();
    let files = [("a.png", "image"), ("b.pdf", "pdf")].map(|(name, text)| AttachmentFile {
        name: name.to_string(),
        bytes: text.as_bytes().to_vec(),
    });
    let source = fx
        .tasks
        .create_task(Some(&json!({ "title": "Speed up search" })), &files, true)
        .unwrap();

    let tries = fan_out(&fx.fan_parts(), &source.id, 2, false).unwrap();

    let kept = list_attachments(&fx.data, &source.id).unwrap();
    for task in &tries {
        assert_eq!(list_attachments(&fx.data, &task.id).unwrap(), kept);
    }
    assert_eq!(kept.len(), 2);
}

#[test]
fn gives_each_try_a_workspace_and_starts_claude_code_in_it() {
    let fx = Fx::without_runner();
    let source = create(&fx, json!({ "title": "Speed up search" }));

    let tries = fan_out(&fx.fan_parts(), &source.id, 2, true).unwrap();

    for task in &tries {
        assert_eq!(task.agent.as_deref(), Some("claude-code"));
        assert!(task.workspace.is_some());
    }
    assert_eq!(fx.pty_count(), 2);
    assert!(fx.pty(0).written()[0].contains("agent-launch-"));
}
