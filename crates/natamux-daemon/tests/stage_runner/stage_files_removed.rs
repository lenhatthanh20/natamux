//! `describe('stage files of a removed workspace')` of `stage-runner.test.ts`.

use std::fs;
use std::path::Path;

use natamux_store::task_files::archive_dir;
use serde_json::json;

use crate::Fixture;

#[test]
fn puts_the_latest_spec_into_the_next_workspace_not_the_last_archived_one() {
    let f = Fixture::new();
    let task = f.created(json!({ "title": "Export tasks" }));
    // Held for review, so no agent starts in the workspaces.
    f.apply(
        &task.id,
        json!({ "source": "human", "type": "state.overridden", "payload": { "to": "review", "stage": "spec" } }),
    );
    fs::write(
        archive_dir(&f.data_dir, &task.id).join("spec.md"),
        "spec v1\n",
    )
    .unwrap();
    let first = f.with_workspace(&task.id);
    let spec = |workspace: &Option<String>| {
        Path::new(workspace.as_deref().unwrap()).join(".natamux-task/spec.md")
    };
    fs::write(spec(&first.workspace), "spec v2\n").unwrap();

    f.remove_workspace(&task.id);
    let next = f.with_workspace(&task.id);

    assert_eq!(
        fs::read_to_string(spec(&next.workspace)).unwrap(),
        "spec v2\n"
    );
}
