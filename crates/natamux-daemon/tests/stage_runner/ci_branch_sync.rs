//! The CI check follows the branch the agent switched the worktree to.

use std::path::{Path, PathBuf};

use natamux_stages::actions::StageAction;
use natamux_stages::ci::CiResult;
use serde_json::json;

use crate::{Fixture, run_git, term_input, wait_for};

#[test]
fn checks_the_branch_the_agent_switched_to() {
    let f = Fixture::new();
    f.set_stages(json!([{ "id": "implement", "kind": "agent", "gate": "manual", "ci": true }]));
    let task = f.implementing("implement");
    f.set_ci(&task.id, "implement", true);
    let worktree = task.workspace.clone().unwrap();
    run_git(Path::new(&worktree), &["switch", "-c", "feature-x"]);

    f.runner
        .run
        .act(&task.id, StageAction::Report, &term_input("1"))
        .unwrap();

    wait_for(|| f.ci.calls.lock().unwrap().len() == 1);
    let calls = f.ci.calls.lock().unwrap().clone();
    assert_eq!(
        calls,
        vec![(PathBuf::from(&worktree), "feature-x".to_string())]
    );
    assert_eq!(f.task(&task.id).branch.as_deref(), Some("feature-x"));
    let started = f.last_of(&task.id, "stage.ci-started").unwrap().payload;
    assert_eq!(started["branch"], "natamux-ci/feature-x");
    let changed = f.last_of(&task.id, "workspace.branch-changed").unwrap();
    assert_eq!(changed.payload["to"], "feature-x");

    f.ci.finish(CiResult {
        passed: true,
        ..CiResult::default()
    });
    f.settle();
}
