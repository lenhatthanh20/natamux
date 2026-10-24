//! `describe('failed side work')` of `stage-runner.test.ts`. The engine logs these failures with
//! `eprintln!`, which a test cannot spy on as Node spies on `console.error`: each test checks
//! the effect that must still hold.

use std::fs;
use std::path::Path;

use natamux_proto::tasks::task::TaskState;
use natamux_store::attachments::AttachmentFile;
use natamux_store::task_files::archive_dir;
use serde_json::json;

use crate::Fixture;
use crate::context::report;

// A "log" case of Node checks `console.error`; the checks here are the real effect.
#[test]
fn logs_a_failed_archive_of_the_stage_files_and_still_starts_the_next_stage() {
    let f = Fixture::new();
    let task = f.implementing("implement");
    // A folder with the name of a stage file cannot be copied.
    fs::create_dir(Path::new(task.workspace.as_deref().unwrap()).join(".natamux-task/x.md"))
        .unwrap();

    report(&f, &task.id, "1");
    f.settle();

    // Implement runs in the step "build" of the default pipeline. Testing comes after it.
    let now = f.task(&task.id);
    assert_eq!(
        (now.stage.as_str(), now.state),
        ("testing", TaskState::Running)
    );
}

#[test]
fn logs_a_failed_restore_of_the_stage_files_and_still_restores_the_attachments_and_starts_the_stage()
 {
    let f = Fixture::new();
    let files = [AttachmentFile {
        name: "login.png".into(),
        bytes: b"image".to_vec(),
    }];
    let input = json!({ "title": "Export", "agent": "claude-code" });
    let task = f.tasks.create_task(Some(&input), &files, true).unwrap();
    f.settle();
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "ready", "stage": "implement" } }),
    );
    fs::create_dir(archive_dir(&f.data_dir, &task.id).join("x.md")).unwrap();

    let after = f.with_workspace(&task.id);

    assert_eq!(
        (after.stage.as_str(), after.state),
        ("build", TaskState::Running)
    );
    let attachment =
        Path::new(after.workspace.as_deref().unwrap()).join(".natamux-task/attachments/login.png");
    assert!(attachment.exists());
}

#[test]
fn logs_a_failed_delete_of_the_ci_branch_of_a_finished_task() {
    let f = Fixture::new();
    // The test repository has no origin, so the push fails.
    let task = f.implementing("implement");
    f.tasks.set_ci(&task.id, "testing", true).unwrap();

    f.apply(
        &task.id,
        json!({ "source": "human", "type": "task.cancelled" }),
    );
    f.settle();

    // The failed delete only logs: the task still ends.
    assert_eq!(f.task(&task.id).state, TaskState::Cancelled);
}

// Left out: 'logs stages that fail to start': it mocks `startLanes` to reject, which a Rust test
// cannot do, and no input makes the real `start_lanes` fail after a report.
