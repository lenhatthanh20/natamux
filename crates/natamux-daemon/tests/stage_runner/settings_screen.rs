//! `describe('settings from the Stages screen')` of `stage-runner.test.ts`.

use natamux_proto::stages::schema::Gate;
use serde_json::json;

use crate::Fixture;
use crate::context::{change_stage, report, write_working};

#[test]
fn moves_on_from_a_stage_that_does_not_wait_for_approval() {
    let f = Fixture::new();
    change_stage(&f, "spec", |stage| stage.gate = Gate::Auto);
    let waiting = f.created(json!({ "title": "Export tasks", "agent": "claude-code" }));
    let task = f.with_workspace(&waiting.id);
    write_working(
        &task,
        "spec.md",
        "# Goal\nx\n# Scope\nx\n# Out of scope\nx\n# Acceptance criteria\nx\n# Code map\nx\n",
    );

    report(&f, &task.id, "1");
    f.settle();
    assert_eq!(f.task(&task.id).stage, "plan");
}

#[test]
fn fills_the_placeholders_of_a_prompt_of_your_own_and_adds_the_done_line_once() {
    let f = Fixture::new();
    change_stage(&f, "spec", |stage| {
        stage.prompt = Some("Spec {title} | {description} | {dir} | {ref}.".into());
    });
    let waiting = f.created(json!({
        "title": "Export tasks",
        "description": "CSV please",
        "agent": "claude-code",
        "context": { "source": "manual", "ref": "#9" },
    }));
    let task = f.with_workspace(&waiting.id);

    let script = f.launch_script(&task.id, "1");
    assert!(script.contains("Spec Export tasks | CSV please | .natamux-task | #9."));
    assert_eq!(script.matches("natamux done").count(), 1);
    assert!(!script.contains("Do not commit"));
}
