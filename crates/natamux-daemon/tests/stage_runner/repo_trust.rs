//! Repo trust: a stage of a project whose commands are not approved runs none of them, and fails
//! with the reason (S-02).

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use natamux_proto::projects::repo_trust::UNTRUSTED_REASON;
use natamux_stages::actions::StageAction;
use natamux_stages::checks::Ran;
use serde_json::json;

use crate::Fixture;
use crate::context::payloads_of;

/// Counts the commands the runner runs itself (checks and path conditions).
fn counting(f: &Fixture) -> Arc<AtomicUsize> {
    let ran = Arc::new(AtomicUsize::new(0));
    let count = Arc::clone(&ran);
    f.set_run_command(move |_, _| {
        count.fetch_add(1, Ordering::SeqCst);
        Ran {
            code: 0,
            output: String::new(),
        }
    });
    ran
}

fn report(f: &Fixture, id: &str) {
    f.runner
        .run
        .act(id, StageAction::Report, &crate::term_input("1"))
        .unwrap();
    f.settle();
}

#[test]
fn fails_a_stage_check_without_running_it() {
    let mut f = Fixture::new();
    f.untrusted();
    let stages = json!([
        { "id": "implement", "kind": "agent", "gate": "manual", "check": "touch ran" },
    ]);
    f.set_stages(stages);
    let ran = counting(&f);
    let task = f.implementing("implement");

    report(&f, &task.id);

    assert_eq!(ran.load(Ordering::SeqCst), 0);
    let failed = json!({ "stage": "implement", "reasons": [UNTRUSTED_REASON], "final": true });
    assert_eq!(payloads_of(&f, &task.id, "stage.check-failed"), [failed]);
}

#[test]
fn fails_a_command_stage_without_typing_its_command() {
    let mut f = Fixture::new();
    f.untrusted();
    let stages = json!([
        { "id": "test", "kind": "testing", "gate": "auto", "command": "touch ran" },
    ]);
    f.set_stages(stages);
    let task = f.implementing("test");
    f.settle();

    let typed: String = f.ptys().iter().flat_map(|pty| pty.written()).collect();
    assert!(!typed.contains("touch ran"), "{typed}");
    let failed = json!({ "stage": "test", "reasons": [UNTRUSTED_REASON], "final": true });
    assert_eq!(payloads_of(&f, &task.id, "stage.check-failed"), [failed]);
}

// Taking the other path would be a silent wrong path.
#[test]
fn fails_the_stage_before_a_branch_whose_path_condition_runs_a_command() {
    let mut f = Fixture::new();
    f.untrusted();
    let stages = json!([
        { "id": "plan", "kind": "agent", "gate": "auto" },
        { "id": "kind", "paths": [
            { "id": "bug", "when": { "type": "command", "run": "touch ran" },
              "stages": [{ "id": "repro", "kind": "agent", "gate": "auto" }] },
            { "id": "feature", "stages": [{ "id": "build", "kind": "agent", "gate": "auto" }] },
        ] },
    ]);
    f.set_stages(stages);
    let ran = counting(&f);
    let task = f.implementing("plan");

    report(&f, &task.id);

    assert_eq!(ran.load(Ordering::SeqCst), 0);
    assert_eq!(f.task(&task.id).stage, "plan");
    let failed = json!({ "stage": "plan", "reasons": [UNTRUSTED_REASON], "final": true });
    assert_eq!(payloads_of(&f, &task.id, "stage.check-failed"), [failed]);
}
